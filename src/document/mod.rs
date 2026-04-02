pub mod docx;
pub mod odt;
pub mod pdf;

pub use odt::translate_odt;
pub use pdf::translate_pdf;

use anyhow::Result;

use crate::api::chunker::{context_size_from_model_id, usable_input_chars};
use crate::api::{chat::translate_single, client::OpenAiClient};

// We use a highly unlikely string as a batch separator because null bytes
// are often stripped by LLM APIs or tokenizers, breaking the split logic.
const PARA_SEP: &str = "[---PARAGRAPH_SEPARATOR---]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pdf,
    Odt,
}

impl OutputFormat {
    /// Parse from a string ("pdf" or "odt"). Returns None if unrecognised.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "odt" => Some(Self::Odt),
            _ => None,
        }
    }

    /// Default output format for a given input extension.
    pub fn default_for(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "pdf" => Self::Pdf,
            _ => Self::Odt, // odt and docx → ODT
        }
    }

    pub fn ext(&self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Odt => "odt",
        }
    }

    pub fn mime(&self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Odt => "application/vnd.oasis.opendocument.text",
        }
    }
}

/// Translate a document (PDF, DOCX, or ODT) and return output bytes in the requested format.
/// Returns `(bytes, output_ext, mime_type)`.
pub async fn translate_document(
    bytes: &[u8],
    input_ext: &str,
    output_fmt: OutputFormat,
    client: &OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<(Vec<u8>, &'static str, &'static str)> {
    let out = match (input_ext.to_lowercase().as_str(), output_fmt) {
        // ODT→ODT preserves original structure; PDF→PDF re-renders (lossy)
        ("odt", OutputFormat::Odt) => {
            translate_odt(bytes, client, model, source_lang, target_lang).await?
        }
        ("pdf", OutputFormat::Pdf) => {
            translate_pdf(bytes, client, model, source_lang, target_lang).await?
        }

        // DOCX → ODT
        ("docx", OutputFormat::Odt) => {
            let paragraphs = docx::extract_docx_paragraphs(bytes)?;
            let translated = translate_paragraphs_sparse(&paragraphs, client, model, source_lang, target_lang).await?;
            odt::build_odt_from_paragraphs(&translated)?
        }

        // DOCX → PDF
        ("docx", OutputFormat::Pdf) => {
            let paragraphs = docx::extract_docx_paragraphs(bytes)?;
            let translated = translate_paragraphs_sparse(&paragraphs, client, model, source_lang, target_lang).await?;
            pdf::build_pdf_from_text(&translated.join("\n"))?
        }

        // ODT → PDF
        ("odt", OutputFormat::Pdf) => {
            let paragraphs = odt::extract_odt_paragraphs(bytes)?;
            let translated = translate_paragraphs_sparse(&paragraphs, client, model, source_lang, target_lang).await?;
            pdf::build_pdf_from_text(&translated.join("\n"))?
        }

        // PDF → ODT
        ("pdf", OutputFormat::Odt) => {
            let text = pdf::extract_pdf_text(bytes)?;
            let (translated, _, _) =
                crate::api::chat::translate(client, model, source_lang, target_lang, &text)
                    .await?;
            odt::build_odt_from_text(&translated)?
        }

        _ => anyhow::bail!("unsupported input format: .{input_ext}"),
    };

    Ok((out, output_fmt.ext(), output_fmt.mime()))
}

/// Translate paragraphs while skipping empty ones and preserving their positions.
///
/// Empty paragraphs are left as empty strings in the output, matching the
/// behaviour of `translate_odt` and ensuring blank-line spacing is preserved
/// across all cross-format conversion paths.
async fn translate_paragraphs_sparse(
    paragraphs: &[String],
    client: &OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<String>> {
    let non_empty_indices: Vec<usize> = paragraphs
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(i, _)| i)
        .collect();

    if non_empty_indices.is_empty() {
        return Ok(paragraphs.iter().map(|_| String::new()).collect());
    }

    let non_empty_refs: Vec<&str> = non_empty_indices.iter().map(|&i| paragraphs[i].as_str()).collect();
    let translated_non_empty =
        translate_paragraphs(&non_empty_refs, client, model, source_lang, target_lang).await?;

    let mut translated: Vec<String> = paragraphs.iter().map(|_| String::new()).collect();
    for (j, &idx) in non_empty_indices.iter().enumerate() {
        translated[idx] = translated_non_empty[j].clone();
    }
    Ok(translated)
}

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
        let cost = if current.is_empty() {
            para_chars
        } else {
            sep_chars + para_chars
        };

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

        let translated =
            translate_single(client, model, source_lang, target_lang, &joined).await?;

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
