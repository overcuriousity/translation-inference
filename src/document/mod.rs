pub mod docx;
pub mod odt;
pub mod pdf;

pub use docx::translate_docx;
pub use odt::translate_odt;
pub use pdf::translate_pdf;

use anyhow::Result;

use crate::api::chunker::{context_size_from_model_id, usable_input_chars};
use crate::api::{chat::translate_single, client::OpenAiClient};

// We use a highly unlikely string as a batch separator because null bytes
// are often stripped by LLM APIs or tokenizers, breaking the split logic.
const PARA_SEP: &str = "[---PARAGRAPH_SEPARATOR---]";

/// Translate a list of paragraph strings, preserving order and count.
///
/// Paragraphs are batched into groups whose combined character count fits
/// within the model's context window, so the separator is never split by the
/// chunker.  A single paragraph that exceeds the window on its own is passed
/// to `translate_single`, which falls back to the chunked path.
pub async fn translate_paragraphs(
    paragraphs: &[&str],
    client: &OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<String>> {
    if paragraphs.is_empty() {
        return Ok(Vec::new());
    }

    // max_chars for joined batch text (separators count toward budget)
    let max_chars = usable_input_chars(context_size_from_model_id(model));
    // "\n\n" + SEP + "\n\n"
    let sep_full = format!("\n\n{PARA_SEP}\n\n");
    let sep_chars = sep_full.chars().count();

    // Build batches of indices whose joined length fits in max_chars.
    let mut batches: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_len: usize = 0;

    for (i, para) in paragraphs.iter().enumerate() {
        let para_chars = para.chars().count();
        let cost = if current.is_empty() { para_chars } else { sep_chars + para_chars };

        if !current.is_empty() && current_len + cost > max_chars {
            batches.push(std::mem::take(&mut current));
            current.push(i);
            current_len = para_chars;
        } else {
            current.push(i);
            current_len += cost;
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }

    let mut results: Vec<String> = vec![String::new(); paragraphs.len()];

    for batch_indices in &batches {
        let joined = batch_indices
            .iter()
            .map(|&i| paragraphs[i])
            .collect::<Vec<_>>()
            .join(&sep_full);

        let translated = translate_single(client, model, source_lang, target_lang, &joined).await?;

        // Split on the separator.  If the model slightly alters surrounding
        // whitespace we still find the marker itself.
        let parts: Vec<&str> = translated.split(PARA_SEP).collect();

        for (j, &idx) in batch_indices.iter().enumerate() {
            results[idx] = parts
                .get(j)
                .map(|s| s.trim_matches('\n').to_string())
                .unwrap_or_default();
        }
    }

    Ok(results)
}
