//! Text to speech through `/v1/audio/speech`, one voice and model per language.

use reqwest::Client;

use super::upstream::{Endpoint, check};
use crate::error::Error;

/// More chunks than this is a text nobody wants read aloud in one go.
const MAX_CHUNKS: usize = 256;

/// Synthesises `text` as MP3, in chunks of at most `chunk_bytes` when that is not 0.
pub async fn synthesize(
    http: &Client,
    ep: &Endpoint,
    voice: &str,
    model: &str,
    text: &str,
    chunk_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let chunks = split_into_chunks(text, (chunk_bytes > 0).then_some(chunk_bytes));
    if chunks.len() > MAX_CHUNKS {
        return Err(Error::invalid("The text is too long to read aloud. Shorten it and try again."));
    }
    let mut audio = Vec::new();
    for chunk in chunks {
        let body = serde_json::json!({ "model": model, "input": chunk, "voice": voice, "response_format": "mp3" });
        let res = check(ep.post(http, "/v1/audio/speech").json(&body).send().await, "speech").await?;
        let bytes = res.bytes().await.map_err(|e| Error::upstream("The speech could not be read.", e))?;
        audio.extend_from_slice(&bytes);
    }
    Ok(audio)
}

/// Split text into chunks of at most `max_bytes` bytes, breaking at sentence
/// boundaries where possible. If `max_bytes` is `None`, the text is returned
/// as a single chunk (suitable for local models with no size limit).
fn split_into_chunks(text: &str, max_bytes: Option<usize>) -> Vec<String> {
    let Some(limit) = max_bytes else {
        return vec![text.to_string()];
    };

    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for sentence in split_sentences(text) {
        if current.len() + sentence.len() > limit && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current = String::new();
        }
        // If a single sentence exceeds the limit, hard-split it.
        if sentence.len() > limit {
            if !current.is_empty() {
                chunks.push(current.trim().to_string());
                current = String::new();
            }
            // Use char_indices so all slice boundaries are valid UTF-8.
            let mut chunk_start = 0usize;
            let mut current_len = 0usize;
            let mut last_space: Option<usize> = None;
            for (i, ch) in sentence.char_indices() {
                let ch_len = ch.len_utf8();
                if current_len + ch_len > limit {
                    // Prefer breaking at the last space; otherwise split before current char.
                    let split_at = match last_space {
                        Some(sp) if sp > chunk_start => sp,
                        _ => i,
                    };
                    let part = sentence[chunk_start..split_at].trim();
                    if !part.is_empty() {
                        chunks.push(part.to_string());
                    }
                    chunk_start = split_at;
                    current_len = sentence[chunk_start..i].len();
                    last_space = None;
                }
                current_len += ch_len;
                if ch == ' ' {
                    last_space = Some(i + ch_len);
                }
            }
            if chunk_start < sentence.len() {
                let part = sentence[chunk_start..].trim();
                if !part.is_empty() {
                    chunks.push(part.to_string());
                }
            }
        } else {
            current.push_str(&sentence);
        }
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Yield sentence-like spans from `text`, preserving trailing whitespace so
/// that rejoining produces the original string.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        // ASCII sentence terminators + fullwidth equivalents used in Japanese, Chinese, Korean.
        if matches!(ch, '.' | '!' | '?' | '。' | '！' | '？' | '…' | '\u{203C}' | '\u{2049}') || ch == '\n' {
            // Include the punctuation and any following whitespace in this span.
            let mut end = i + 1;
            while end < len && (chars[end] == ' ' || chars[end] == '\u{3000}') {
                end += 1;
            }
            let span: String = chars[start..end].iter().collect();
            sentences.push(span);
            start = end;
            i = end;
        } else {
            i += 1;
        }
    }

    if start < len {
        sentences.push(chars[start..].iter().collect());
    }

    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── split_into_chunks ─────────────────────────────────────────────────────

    const TEST_LIMIT: usize = 4000;

    #[test]
    fn short_text_is_single_chunk() {
        let chunks = split_into_chunks("Hello world.", Some(TEST_LIMIT));
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn no_limit_is_single_chunk() {
        let text = "x".repeat(TEST_LIMIT * 10);
        let chunks = split_into_chunks(&text, None);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn long_text_splits_at_sentence_boundary() {
        let sentence = "This is a sentence. ";
        let text = sentence.repeat((TEST_LIMIT / sentence.len()) + 5);
        let chunks = split_into_chunks(&text, Some(TEST_LIMIT));
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= TEST_LIMIT);
        }
    }

    // ── CJK / fullwidth punctuation ───────────────────────────────────────────

    #[test]
    fn japanese_fullwidth_period_splits_sentences() {
        // '。' is U+3002 IDEOGRAPHIC FULL STOP — should act as a sentence boundary.
        let sentences = split_sentences("これはテストです。次の文です。");
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].ends_with('。'));
        assert!(sentences[1].ends_with('。'));
    }

    #[test]
    fn cjk_text_splits_into_chunks() {
        // Each Japanese sentence ends with '。'. Repeat until we exceed TEST_LIMIT.
        let sentence = "これは長いテキストのテストです。";
        let text = sentence.repeat((TEST_LIMIT / sentence.len()) + 5);
        let chunks = split_into_chunks(&text, Some(TEST_LIMIT));
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= TEST_LIMIT);
        }
    }

    #[test]
    fn ideographic_space_consumed_after_terminator() {
        // U+3000 IDEOGRAPHIC SPACE following a terminator should be absorbed into
        // the preceding span, not left at the start of the next one.
        let text = "文A。\u{3000}文B。";
        let sentences = split_sentences(text);
        assert_eq!(sentences.len(), 2);
        // The ideographic space belongs to the first span.
        assert!(sentences[0].ends_with('\u{3000}'));
        assert!(sentences[1].starts_with('文'));
    }
}
