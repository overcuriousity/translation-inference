use axum::{extract::State, http::{HeaderMap, StatusCode}, Json};
use std::sync::Arc;

use crate::api::{chat, chunker::{context_size_from_model_id, usable_input_chars, TranslationConfig}};
use crate::models::{ErrorResponse, ParagraphPair, TranslateParagraphsRequest, TranslateParagraphsResponse};
use crate::routes::translate::{get_char_limit, resolve_translation_client};
use crate::AppState;

/// Separator injected between source paragraphs before sending to the LLM.
/// Chosen to be unlikely to appear in normal text.
const SEP: &str = "\n§§§\n";

pub async fn post_translate_paragraphs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TranslateParagraphsRequest>,
) -> Result<Json<TranslateParagraphsResponse>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(limit) = get_char_limit(&state, &headers) {
        let len = req.text.chars().count();
        if len > limit {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ErrorResponse {
                    error: format!("Input is {len} characters, which exceeds the {limit}-character limit for your access tier."),
                }),
            ));
        }
    }

    if req.text.trim().is_empty() {
        return Ok(Json(TranslateParagraphsResponse {
            paragraphs: vec![],
            chunks_total: 0,
            chunks_completed: 0,
        }));
    }

    let client = resolve_translation_client(&state, req.endpoint.as_deref(), req.api_key.as_deref(), &headers)?;

    let model = req
        .model
        .as_deref()
        .unwrap_or(&state.config.translation_model);

    // Normalize newlines so paragraph splitting works for both Unix and Windows inputs.
    let normalized_text = req.text.replace("\r\n", "\n").replace('\r', "\n");
    // Split source into paragraphs (split on blank lines).
    let source_paragraphs: Vec<&str> = normalized_text.split("\n\n").collect();

    // Collect non-empty paragraphs and their original indices.
    let non_empty: Vec<(usize, &str)> = source_paragraphs
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.trim().is_empty())
        .map(|(i, p)| (i, *p))
        .collect();

    if non_empty.is_empty() {
        let paragraphs = source_paragraphs
            .iter()
            .map(|p| ParagraphPair {
                source: p.to_string(),
                translation: String::new(),
            })
            .collect();
        return Ok(Json(TranslateParagraphsResponse {
            paragraphs,
            chunks_total: 0,
            chunks_completed: 0,
        }));
    }

    // Join non-empty paragraphs with separator and translate as one request.
    let joined: String = non_empty.iter().map(|(_, p)| *p).collect::<Vec<_>>().join(SEP);

    let config = TranslationConfig::from(&state.config);

    // Guard: if the joined text would require chunking the §§§ separators could
    // be split across chunks, misaligning paragraphs.  Reject inputs that exceed
    // the single-pass budget and let the caller reduce the text or use the plain
    // translation endpoint instead.
    let max_chars = usable_input_chars(context_size_from_model_id(model, &config), &joined, &config);
    if joined.chars().count() > max_chars {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse {
                error: format!(
                    "Input is too large for paragraph-aligned translation \
                     ({} chars, limit ~{} chars). Please shorten the text or use \
                     the regular translation endpoint.",
                    joined.chars().count(),
                    max_chars
                ),
            }),
        ));
    }

    let source_lang = req.source_lang.as_deref().unwrap_or("auto");
    let translated = chat::translate_single(
        &client,
        model,
        source_lang,
        &req.target_lang,
        &joined,
        req.context.as_deref(),
        &config,
    )
    .await
    .map_err(|e| {
        tracing::error!("Paragraph translation error: {e:#}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() }),
        )
    })?;
    let (chunks_total, chunks_completed) = (1usize, 1usize);

    // Split translated text back on the separator; strip empty leading/trailing splits
    // that can arise when the model wraps the output in extra newlines.
    let mut translated_parts: Vec<&str> = translated
        .split("§§§")
        .map(str::trim)
        .collect();
    while translated_parts.first().is_some_and(|p| p.is_empty()) {
        translated_parts.remove(0);
    }
    while translated_parts.last().is_some_and(|p| p.is_empty()) {
        translated_parts.pop();
    }

    if translated_parts.len() != non_empty.len() {
        let error = format!(
            "Paragraph translation separator mismatch: expected {} parts, got {}. \
             The model may have dropped or altered the §§§ separator.",
            non_empty.len(),
            translated_parts.len()
        );
        tracing::error!("{error}");
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error })));
    }

    // Build result array preserving original paragraph positions.
    let mut results: Vec<String> = source_paragraphs.iter().map(|_| String::new()).collect();
    for (slot, (orig_idx, _)) in non_empty.iter().enumerate() {
        results[*orig_idx] = translated_parts[slot].to_string();
    }

    let paragraphs = source_paragraphs
        .iter()
        .zip(results.iter())
        .map(|(src, tgt)| ParagraphPair {
            source: src.to_string(),
            translation: tgt.clone(),
        })
        .collect();

    Ok(Json(TranslateParagraphsResponse {
        paragraphs,
        chunks_total,
        chunks_completed,
    }))
}
