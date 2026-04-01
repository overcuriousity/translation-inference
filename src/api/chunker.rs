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
    // Try to find a large number at the end of the model id
    model_id
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<usize>().ok())
        .filter(|&n| n >= 1024)
        .last()
        .unwrap_or(4096)
}

/// A single chunk together with optional overlap text that precedes it.
#[derive(Debug)]
pub struct Chunk {
    /// The text to be translated.
    pub text: String,
    /// The last sentence(s) of the *previous* chunk, used as context.
    pub overlap: Option<String>,
}

/// Split `text` into chunks that fit within `max_chars`, adding a small
/// overlap of previous context for coherence.
pub fn split_into_chunks(text: &str, max_chars: usize) -> Vec<Chunk> {
    if text.chars().count() <= max_chars {
        return vec![Chunk { text: text.to_string(), overlap: None }];
    }

    // Reserve some space for the overlap prefix in the prompt (~10%)
    let chunk_budget = (max_chars * 9) / 10;

    let segments = split_at_boundaries(text, chunk_budget);

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut prev_overlap: Option<String> = None;

    for seg in segments {
        chunks.push(Chunk {
            text: seg.clone(),
            overlap: prev_overlap.clone(),
        });
        // Use the last 1-2 sentences of this segment as overlap for the next
        prev_overlap = Some(last_sentences(&seg, 2));
    }

    chunks
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

        // Find a good split point within the first max_chars characters
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
    // Convert max_chars to a byte boundary (chars can be multi-byte)
    let byte_limit = text
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(text.len());

    let candidate = &text[..byte_limit];

    // Paragraph boundary (prefer the last one)
    if let Some(pos) = candidate.rfind("\n\n") {
        return pos + 2;
    }

    // Sentence boundary: ". " or ".\n" or "! " or "? "
    for delim in &[". ", ".\n", "! ", "? ", "。"] {
        if let Some(pos) = candidate.rfind(delim) {
            return pos + delim.len();
        }
    }

    // Word boundary
    if let Some(pos) = candidate.rfind(' ') {
        return pos + 1;
    }

    // Hard cut at byte_limit (rare)
    byte_limit
}

/// Extract the last `n` sentences from a text block (for overlap).
fn last_sentences(text: &str, n: usize) -> String {
    // Split on sentence-ending punctuation
    let mut ends: Vec<usize> = Vec::new();
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'.' || b == b'!' || b == b'?') && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            ends.push(i + 2);
        }
    }

    if ends.is_empty() {
        // No sentence boundaries found; just return the last chunk of chars
        let start = text
            .char_indices()
            .rev()
            .nth(200)
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
        assert!(chunks[0].overlap.is_none());
    }

    #[test]
    fn test_chunking_with_overlap() {
        let long_text = "First sentence. Second sentence. Third sentence. Fourth sentence. Fifth sentence.";
        let chunks = split_into_chunks(long_text, 40);
        assert!(chunks.len() > 1);
        // Second chunk should have overlap from the first
        if chunks.len() > 1 {
            assert!(chunks[1].overlap.is_some());
        }
    }

    #[test]
    fn test_lossless_chunking() {
        let text = "Line 1.\n\nLine 2. Sentence part 1. Part 2.\nLine 3 with  multiple   spaces.";
        let chunks = split_into_chunks(text, 10);
        let reconstructed: String = chunks.into_iter().map(|c| c.text).collect();
        assert_eq!(reconstructed, text);
    }
}
