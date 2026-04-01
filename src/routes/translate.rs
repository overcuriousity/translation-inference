use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use crate::api::{chat, client::OpenAiClient};
use crate::models::{ErrorResponse, TranslateRequest, TranslateResponse};
use crate::AppState;

pub async fn post_translate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TranslateRequest>,
) -> Result<Json<TranslateResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.text.trim().is_empty() {
        return Ok(Json(TranslateResponse {
            translated_text: String::new(),
            chunks_total: 0,
            chunks_completed: 0,
        }));
    }

    let client = resolve_client(&state, req.endpoint.as_deref(), req.api_key.as_deref())?;

    let model = req
        .model
        .as_deref()
        .unwrap_or(&state.config.translation_model);

    match chat::translate(&client, model, &req.source_lang, &req.target_lang, &req.text).await {
        Ok((translated_text, chunks_total, chunks_completed)) => Ok(Json(TranslateResponse {
            translated_text,
            chunks_total,
            chunks_completed,
        })),
        Err(e) => {
            tracing::error!("Translation error: {e:#}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e.to_string() }),
            ))
        }
    }
}

pub fn resolve_client(
    state: &AppState,
    endpoint: Option<&str>,
    api_key: Option<&str>,
) -> Result<OpenAiClient, (StatusCode, Json<ErrorResponse>)> {
    if let (Some(ep), Some(key)) = (endpoint, api_key) {
        if !ep.is_empty() && !key.is_empty() {
            return Ok(OpenAiClient::with_credentials(ep, key));
        }
    }
    if state.config.is_configured() {
        Ok(state.client.clone())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "No API credentials configured. Provide endpoint and api_key in the request.".into(),
            }),
        ))
    }
}
