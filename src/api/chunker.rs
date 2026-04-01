fn tokens_to_chars(tokens: usize) -> usize {
    tokens * 4
}

/// Compute the usable input token budget for a given model context size.
/// We reserve 500 tokens for system prompt overhead and leave half the
/// remaining space for the output (translations can be longer than the source).
pub fn usable_input_chars(context_size: usize) -> usize {
    let overhead = 500;
    let usable_tokens = if context_size > overhead {
        (context_size - overhead) / 2
    } else {
        1024
    };
    tokens_to_chars(usable_tokens)
}

/// Parse the context window size from a model ID that encodes it as a suffix,
/// e.g. "gpgpu/qwen3:14b-q5_k_m-32768" → 32768.
/// Falls back to a conservative 4096 if not found.
pub fn context_size_from_model_id(model_id: &str) -> usize {
    model_id
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<usize>().ok())
        .filter(|&n| n >= 1024)
        .last()
        .unwrap_or(4096)
}

/// A single source-text segment to be translated.
#[derive(Debug)]
pub struct Chunk {
    pub text: String,
}

/// Split `text` into chunks that fit within `max_chars`.
/// 10% of the budget is reserved for the translated-overlap prefix added by
/// the caller.
pub fn split_into_chunks(text: &str, max_chars: usize) -> Vec<Chunk> {
    if text.chars().count() <= max_chars {
        return vec![Chunk { text: text.to_string() }];
    }

    // Reserve space for the overlap prefix injected at translation time (~10%)
    let chunk_budget = (max_chars * 9) / 10;

    split_at_boundaries(text, chunk_budget)
        .into_iter()
        .map(|seg| Chunk { text: seg })
        .collect()
}

/// Extract the last `n` sentences from a (translated) text block.
/// Used by the translation layer to derive the overlap passed to the next chunk.
pub fn last_sentences(text: &str, n: usize) -> String {
    let mut ends: Vec<usize> = Vec::new();
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        // ASCII sentence-ending punctuation followed by a space
        if (b == b'.' || b == b'!' || b == b'?') && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            ends.push(i + 2);
        }
        // CJK ideographic full stop (。= 0xE3 0x80 0x82); check 3-byte sequence
        if i + 2 < bytes.len() && bytes[i] == 0xE3 && bytes[i + 1] == 0x80 && bytes[i + 2] == 0x82
        {
            ends.push(i + 3);
        }
    }

    if ends.is_empty() {
        // No sentence boundaries — fall back to last ~200 characters
        let start = text
            .char_indices()
            .rev()
            .nth(199)
            .map(|(i, _)| i)
            .unwrap_or(0);
        return text[start..].to_string();
    }

    let start_idx = if ends.len() >= n {
        ends[ends.len() - n]
    } else {
        0
    };

    text[start_idx..].to_string()
}

/// Split text into segments of at most `max_chars`, preferring splits at
/// paragraph boundaries, then sentence boundaries, then word boundaries.
fn split_at_boundaries(text: &str, max_chars: usize) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.chars().count() <= max_chars {
            result.push(remaining.to_string());
            break;
        }

        let split_at = find_split_point(remaining, max_chars);
        let (chunk, rest) = remaining.split_at(split_at);
        result.push(chunk.to_string());
        remaining = rest;
    }

    result
}

/// Find the best byte index to split `text` at, within `max_chars` characters.
/// Prefers paragraph boundaries > sentence boundaries > word boundaries > hard cut.
fn find_split_point(text: &str, max_chars: usize) -> usize {
    let byte_limit = text
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(text.len());

    let candidate = &text[..byte_limit];

    // Paragraph boundary
    if let Some(pos) = candidate.rfind("\n\n") {
        return pos + 2;
    }

    // Sentence boundary: pick the rightmost match across all delimiters so we
    // don't discard a later boundary just because an earlier delimiter type
    // matched first (e.g. "Dr. Smith ... Great!" should split after "!").
    let best_sentence = [". ", ".\n", "! ", "? ", "。", "！", "？"]
        .iter()
        .filter_map(|d| candidate.rfind(d).map(|p| p + d.len()))
        .max();
    if let Some(pos) = best_sentence {
        return pos;
    }

    // Word boundary
    if let Some(pos) = candidate.rfind(' ') {
        return pos + 1;
    }

    // Hard cut
    byte_limit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_size_from_model_id() {
        assert_eq!(context_size_from_model_id("gpgpu/qwen3:14b-q5_k_m-32768"), 32768);
        assert_eq!(context_size_from_model_id("gpgpu/qwen3-5:9b-q5_k_m-40960"), 40960);
        assert_eq!(context_size_from_model_id("deepseek-chat"), 4096);
    }

    #[test]
    fn test_no_chunking_needed() {
        let text = "Hello world.";
        let chunks = split_into_chunks(text, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
    }

    #[test]
    fn test_chunking_splits_into_multiple() {
        let long_text = "First sentence. Second sentence. Third sentence. Fourth sentence. Fifth sentence.";
        let chunks = split_into_chunks(long_text, 40);
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_lossless_chunking() {
        let text = "Line 1.\n\nLine 2. Sentence part 1. Part 2.\nLine 3 with  multiple   spaces.";
        let chunks = split_into_chunks(text, 10);
        let reconstructed: String = chunks.into_iter().map(|c| c.text).collect();
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn test_last_sentences_ascii() {
        let text = "First sentence. Second sentence. Third sentence.";
        let result = last_sentences(text, 2);
        assert_eq!(result, "Second sentence. Third sentence.");
    }

    #[test]
    fn test_last_sentences_fallback() {
        // No sentence-ending punctuation followed by space
        let text = "no punctuation here";
        let result = last_sentences(text, 2);
        assert!(!result.is_empty());
    }
}
