use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use std::sync::Arc;

use crate::models::{
    ChatMessage, ChatRequest, ChatResponse, DetectLanguageRequest, DetectLanguageResponse,
    ErrorResponse,
};
use crate::routes::translate::{check_authenticated, resolve_client};
use crate::AppState;

pub async fn post_detect_language(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DetectLanguageRequest>,
) -> Result<Json<DetectLanguageResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_authenticated(&state, &headers)?;

    let client = resolve_client(&state, req.endpoint.as_deref(), req.api_key.as_deref(), &headers)?;

    let snippet: String = req.text.chars().take(500).collect();
    if snippet.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "text is required".into() }),
        ));
    }

    let model = state.config.translation_model.clone();

    let chat_req = ChatRequest {
        model,
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "Identify the language of the user's text. Respond with ONLY the ISO 639-1 language code (e.g. 'en', 'de', 'fr', 'zh'). For Traditional Chinese respond with 'zh-TW'. Output nothing else — no punctuation, no explanation.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: snippet,
            },
        ],
        temperature: 0.0,
        max_tokens: Some(10),
        stream: None,
    };

    let resp = client
        .http
        .post(client.chat_url())
        .bearer_auth(&client.api_key)
        .json(&chat_req)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Language detection request failed: {e:#}");
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse { error: format!("Detection request failed: {e}") }),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::error!("Language detection upstream error {status}: {body}");
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: format!("Detection upstream error: {status}") }),
        ));
    }

    let chat: ChatResponse = resp.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Failed to parse detection response: {e}") }),
        )
    })?;

    let raw = chat
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();

    // Normalise: strip whitespace, backticks, quotes, trailing punctuation
    let cleaned = raw.trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('.');

    // Lowercase for comparison, but restore canonical casing for region subtags
    let language = match cleaned.to_lowercase().as_str() {
        "zh-tw" => "zh-TW".to_string(),
        other   => other.to_string(),
    };

    Ok(Json(DetectLanguageResponse { language }))
}
