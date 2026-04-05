use axum::{extract::State, http::{HeaderMap, StatusCode}, Json};
use std::sync::Arc;

use crate::api::{chat, chunker::TranslationConfig};
use crate::models::{ErrorResponse, ParagraphPair, TranslateParagraphsRequest, TranslateParagraphsResponse};
use crate::routes::translate::resolve_client;
use crate::AppState;

/// Separator injected between source paragraphs before sending to the LLM.
/// Chosen to be unlikely to appear in normal text.
const SEP: &str = "\n§§§\n";

pub async fn post_translate_paragraphs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TranslateParagraphsRequest>,
) -> Result<Json<TranslateParagraphsResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.text.trim().is_empty() {
        return Ok(Json(TranslateParagraphsResponse {
            paragraphs: vec![],
            chunks_total: 0,
            chunks_completed: 0,
        }));
    }

    let client = resolve_client(&state, req.endpoint.as_deref(), req.api_key.as_deref(), &headers)?;

    let model = req
        .model
        .as_deref()
        .unwrap_or(&state.config.translation_model);

    // Split source into paragraphs (split on blank lines).
    let source_paragraphs: Vec<&str> = req.text.split("\n\n").collect();

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
    let (translated, chunks_total, chunks_completed) = chat::translate(
        &client,
        model,
        &req.source_lang,
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

    // Split translated text back on the separator.
    let translated_parts: Vec<&str> = translated.split("§§§").collect();

    // Build result array preserving original paragraph positions.
    let mut results: Vec<String> = source_paragraphs.iter().map(|_| String::new()).collect();
    for (slot, (orig_idx, _)) in non_empty.iter().enumerate() {
        results[*orig_idx] = translated_parts
            .get(slot)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
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
