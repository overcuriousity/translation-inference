use crate::config::AppConfig;

const OVERHEAD_TOKENS: usize = 500; // system prompt + chat-format special tokens

/// Runtime-configurable parameters for token budgeting and chunking.
#[derive(Debug, Clone)]
pub struct TranslationConfig {
    /// Fallback context size when the model ID does not encode one (tokens).
    pub default_context_size: usize,
    /// Fraction of usable tokens (after overhead) reserved for input. The
    /// remainder goes to output.  0.5 means a 50/50 split (current default).
    /// Use 0.4 to give 60 % of the budget to output for compact→verbose pairs.
    pub input_token_ratio: f64,
    /// Chars-per-token estimate for CJK-dominant text (Qwen tokeniser: ~1–2).
    pub cjk_chars_per_token: f64,
    /// Chars-per-token estimate for Latin-script text (~3–5 chars/token).
    pub latin_chars_per_token: f64,
    /// Warn when output/input char ratio is below this threshold.
    pub min_output_ratio: f64,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            default_context_size: 4096,
            input_token_ratio: 0.5,
            cjk_chars_per_token: 1.5,
            latin_chars_per_token: 4.0,
            min_output_ratio: 0.3,
        }
    }
}

impl From<&AppConfig> for TranslationConfig {
    fn from(cfg: &AppConfig) -> Self {
        Self {
            default_context_size: cfg.default_context_size,
            input_token_ratio: cfg.input_token_ratio,
            cjk_chars_per_token: cfg.cjk_chars_per_token,
            latin_chars_per_token: cfg.latin_chars_per_token,
            min_output_ratio: cfg.min_output_ratio,
        }
    }
}

/// Estimate the blended chars-per-token ratio for `text` by sampling the
/// first 1000 characters and measuring what fraction are CJK codepoints.
fn chars_per_token_for_text(text: &str, config: &TranslationConfig) -> f64 {
    let sample: Vec<char> = text.chars().take(1000).collect();
    if sample.is_empty() {
        return config.latin_chars_per_token;
    }
    let cjk_count = sample.iter().filter(|&&c| is_cjk(c)).count();
    let cjk_fraction = cjk_count as f64 / sample.len() as f64;
    cjk_fraction * config.cjk_chars_per_token + (1.0 - cjk_fraction) * config.latin_chars_per_token
}

/// Returns true for CJK unified ideographs, extension blocks, and kana/hangul.
#[inline]
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}' |  // Hiragana + Katakana
        '\u{3400}'..='\u{4DBF}' |  // CJK Extension A
        '\u{4E00}'..='\u{9FFF}' |  // CJK Unified Ideographs
        '\u{AC00}'..='\u{D7AF}' |  // Hangul Syllables
        '\u{F900}'..='\u{FAFF}'    // CJK Compatibility Ideographs
    )
}

/// Convert a token budget to a character budget using the text's script profile.
fn tokens_to_chars(tokens: usize, text: &str, config: &TranslationConfig) -> usize {
    let ratio = chars_per_token_for_text(text, config);
    (tokens as f64 * ratio) as usize
}

/// Parse the context window size from a model ID that encodes it as a suffix,
/// e.g. "gpgpu/qwen3:14b-q5_k_m-32768" → 32768.
/// Falls back to `config.default_context_size` if not found.
pub fn context_size_from_model_id(model_id: &str, config: &TranslationConfig) -> usize {
    model_id
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<usize>().ok())
        .filter(|&n| n >= 1024)
        .last()
        .unwrap_or(config.default_context_size)
}

/// Compute the usable input character budget for a given model context size.
/// Reserves OVERHEAD_TOKENS for the system prompt and chat-format tokens,
/// then allocates `config.input_token_ratio` of the remainder to input.
/// Uses a script-aware chars-per-token ratio based on the supplied `text`.
pub fn usable_input_chars(context_size: usize, text: &str, config: &TranslationConfig) -> usize {
    let usable_tokens = if context_size > OVERHEAD_TOKENS {
        ((context_size - OVERHEAD_TOKENS) as f64 * config.input_token_ratio) as usize
    } else {
        1024
    };
    tokens_to_chars(usable_tokens, text, config)
}

/// The explicit max_tokens value to send with each chat request so that
/// LiteLLM / other proxies do not assume the entire context window is available
/// for output (which would cause them to truncate the input to near-zero).
pub fn max_output_tokens(context_size: usize, config: &TranslationConfig) -> u32 {
    if context_size > OVERHEAD_TOKENS {
        ((context_size - OVERHEAD_TOKENS) as f64 * (1.0 - config.input_token_ratio)) as u32
    } else {
        1024
    }
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
        return vec![Chunk {
            text: text.to_string(),
        }];
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
/// Recognises ASCII sentence-ending punctuation and full CJK sentence-ending
/// punctuation (。！？).
pub fn last_sentences(text: &str, n: usize) -> String {
    let mut ends: Vec<usize> = Vec::new();

    for (i, ch) in text.char_indices() {
        let next_byte = i + ch.len_utf8();
        match ch {
            '.' | '!' | '?' => {
                // ASCII punctuation must be followed by a space to count as a
                // sentence end (avoids splitting on "Dr." or "3.14").
                if next_byte < text.len() && text.as_bytes()[next_byte] == b' ' {
                    ends.push(next_byte + 1);
                }
            }
            // CJK sentence-ending punctuation: only add if there is more text
            // after the terminator, mirroring the ASCII behaviour of requiring
            // a trailing space. This avoids counting the final terminator as a
            // sentence boundary which would make ends.len() - n overshoot.
            '\u{3002}' | '\u{FF01}' | '\u{FF1F}' => {
                if next_byte < text.len() {
                    ends.push(next_byte);
                }
            }
            _ => {}
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

    fn cfg() -> TranslationConfig {
        TranslationConfig::default()
    }

    #[test]
    fn test_context_size_from_model_id() {
        assert_eq!(
            context_size_from_model_id("gpgpu/qwen3:14b-q5_k_m-32768", &cfg()),
            32768
        );
        assert_eq!(
            context_size_from_model_id("gpgpu/qwen3-5:9b-q5_k_m-40960", &cfg()),
            40960
        );
        assert_eq!(context_size_from_model_id("deepseek-chat", &cfg()), 4096);
    }

    #[test]
    fn test_context_size_from_model_id_custom_default() {
        let mut c = cfg();
        c.default_context_size = 8192;
        assert_eq!(context_size_from_model_id("deepseek-chat", &c), 8192);
        // A model with an explicit size still wins over the default.
        assert_eq!(context_size_from_model_id("some-model-32768", &c), 32768);
    }

    #[test]
    fn test_no_chunking_needed() {
        let text = "Hello world.";
        let max = usable_input_chars(32768, text, &cfg());
        let chunks = split_into_chunks(text, max);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
    }

    #[test]
    fn test_chunking_splits_into_multiple() {
        let long_text =
            "First sentence. Second sentence. Third sentence. Fourth sentence. Fifth sentence.";
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

    #[test]
    fn test_last_sentences_cjk_exclamation() {
        let text = "第一句话！第二句话！第三句话！";
        let result = last_sentences(text, 2);
        // Should return the last 2 CJK sentences
        assert!(result.contains("第二句话！"));
        assert!(result.contains("第三句话！"));
        assert!(!result.contains("第一句话！"));
    }

    #[test]
    fn test_last_sentences_cjk_question() {
        let text = "你好吗？我很好。真的吗？";
        let result = last_sentences(text, 2);
        assert!(result.contains("真的吗？"));
        assert!(result.contains("我很好。"));
        assert!(!result.contains("你好吗？"));
    }

    #[test]
    fn test_chars_per_token_latin() {
        let text = "Hello world, this is plain Latin text with no CJK characters at all.";
        let ratio = chars_per_token_for_text(text, &cfg());
        // Should be close to latin_chars_per_token (4.0)
        assert!((ratio - 4.0).abs() < 0.1);
    }

    #[test]
    fn test_chars_per_token_cjk() {
        let text = "这是一段纯中文文字，没有任何拉丁字符，用于测试CJK分词比率估算功能。";
        let ratio = chars_per_token_for_text(text, &cfg());
        // Should be close to cjk_chars_per_token (1.5), well below 4.0
        assert!(ratio < 3.0);
        assert!(ratio >= 1.0);
    }

    #[test]
    fn test_chars_per_token_mixed() {
        // 50% CJK, 50% Latin
        let text: String = "AB".repeat(250) + &"中文".repeat(250);
        let ratio = chars_per_token_for_text(&text, &cfg());
        // Blended ratio should be between the two extremes
        assert!(ratio > 1.5);
        assert!(ratio < 4.0);
    }

    #[test]
    fn test_usable_input_chars_cjk_vs_latin() {
        let latin = "The quick brown fox jumps over the lazy dog. ".repeat(20);
        let cjk = "快速的棕色狐狸跳过了懒狗。".repeat(20);
        let ctx = 32768;
        let latin_budget = usable_input_chars(ctx, &latin, &cfg());
        let cjk_budget = usable_input_chars(ctx, &cjk, &cfg());
        // CJK text should get a smaller char budget (fewer chars fit in same token count)
        assert!(cjk_budget < latin_budget);
    }
}
