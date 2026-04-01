use anyhow::{Context, Result};
use regex::Regex;

use crate::api::chunker::{context_size_from_model_id, split_into_chunks, usable_input_chars};
use crate::api::client::OpenAiClient;
use crate::models::{ChatMessage, ChatRequest, ChatResponse};

/// Strip `<think>...</think>` blocks that Qwen3 models sometimes emit.
fn strip_think_tags(s: &str) -> String {
    // Lazy match to handle multiple think blocks
    let re = Regex::new(r"(?s)<think>.*?</think>").unwrap();
    re.replace_all(s, "").trim().to_string()
}

fn build_system_prompt(source_lang: &str, target_lang: &str) -> String {
    let source_clause = if source_lang.eq_ignore_ascii_case("auto") {
        "Detect the source language automatically.".to_string()
    } else {
        format!("The source language is {source_lang}.")
    };

    format!(
        "You are a professional, literal translator. {source_clause} Translate the user's text to {target_lang}.\n\
         Rules:\n\
         - Translate the ENTIRE text exactly. Do not summarize, do not omit any sections, and do not skip repetitive content.\n\
         - If the input is in a structured format (JSON, XML, code, logs), preserve the EXACT structure and translate only the natural language values.\n\
         - Output ONLY the translated content, nothing else (no headers, no labels, no 'Here is the translation').\n\
         - Preserve all original formatting (paragraphs, line breaks, indentation, whitespace).\n\
         - Preserve proper nouns and technical identifiers unless they have a standard translation.\n\
         - Match the register and tone precisely."
    )
}

/// Translate a single piece of text (no chunking).
async fn translate_chunk(
    client: &OpenAiClient,
    model: &str,
    system_prompt: &str,
    overlap: Option<&str>,
    text: &str,
) -> Result<String> {
    let user_content = match overlap {
        Some(ctx) => format!(
            "CONTEXT FROM PREVIOUS SECTION (DO NOT TRANSLATE THIS):\n{ctx}\n\nTEXT TO TRANSLATE (TRANSLATE EVERYTHING BELOW EXACTLY):\n{text}"
        ),
        None => text.to_string(),
    };

    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage { role: "system".to_string(), content: system_prompt.to_string() },
            ChatMessage { role: "user".to_string(), content: user_content },
        ],
        temperature: 0.3,
        max_tokens: None,
    };

    let response = client
        .http
        .post(client.chat_url())
        .bearer_auth(&client.api_key)
        .json(&request)
        .send()
        .await
        .context("failed to send chat completion request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("chat API error {status}: {body}");
    }

    let chat: ChatResponse = response
        .json()
        .await
        .context("failed to parse chat completion response")?;

    let raw = chat
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();

    Ok(strip_think_tags(&raw))
}

/// Translate `text`, automatically chunking if it exceeds the model's context window.
/// Returns `(translated_text, chunks_total, chunks_completed)`.
pub async fn translate(
    client: &OpenAiClient,
    model: &str,
    source_lang: &str,
    target_lang: &str,
    text: &str,
) -> Result<(String, usize, usize)> {
    let context_size = context_size_from_model_id(model);
    let max_chars = usable_input_chars(context_size);

    let chunks = split_into_chunks(text, max_chars);
    let total = chunks.len();
    let system_prompt = build_system_prompt(source_lang, target_lang);

    let mut parts: Vec<String> = Vec::with_capacity(total);

    for chunk in &chunks {
        let translated = translate_chunk(
            client,
            model,
            &system_prompt,
            chunk.overlap.as_deref(),
            &chunk.text,
        )
        .await?;
        parts.push(translated);
    }

    Ok((parts.concat(), total, total))
}
