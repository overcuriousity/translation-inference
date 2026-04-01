use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    models::{ErrorResponse, SaveToBitvaultRequest, SaveToBitvaultResponse},
    AppState,
};

// ── Save translation result to Bitvault ─────────────────────────────────────

#[derive(Serialize)]
struct BitvaultCreateBody<'a> {
    content: &'a str,
    expiration: &'static str,
    privacy: &'static str,
}

#[derive(Deserialize)]
struct BitvaultCreateResp {
    url: String,
}

pub async fn post_save_to_bitvault(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveToBitvaultRequest>,
) -> Result<Json<SaveToBitvaultResponse>, (StatusCode, Json<ErrorResponse>)> {
    let bitvault_url = state.config.bitvault_url.as_deref().ok_or_else(|| (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Bitvault not configured".into() }),
    ))?;

    let endpoint = format!("{bitvault_url}/api/v1/paste");
    let body = BitvaultCreateBody {
        content: &req.text,
        expiration: "never",
        privacy: "unlisted",
    };

    let mut builder = state.client.http.post(&endpoint).json(&body);
    if let Some(key) = &state.config.bitvault_api_key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }

    let resp = builder.send().await.map_err(|e| (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse { error: format!("Failed to reach Bitvault: {e}") }),
    ))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: format!("Bitvault returned {status}: {text}") }),
        ));
    }

    let paste: BitvaultCreateResp = resp.json().await.map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { error: format!("Failed to parse Bitvault response: {e}") }),
    ))?;

    Ok(Json(SaveToBitvaultResponse { url: paste.url }))
}

// ── Proxy raw text from Bitvault (avoids browser CORS) ──────────────────────

#[derive(Deserialize)]
pub struct ProxyQuery {
    pub url: String,
}

pub async fn get_proxy_text(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ProxyQuery>,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let bitvault_url = state.config.bitvault_url.as_deref().ok_or_else(|| (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse { error: "Bitvault not configured".into() }),
    ))?;

    if !q.url.starts_with(bitvault_url) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse { error: "URL not allowed — must point to the configured Bitvault instance".into() }),
        ));
    }

    let text = state.client.http
        .get(&q.url)
        .send()
        .await
        .map_err(|e| (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: format!("Failed to fetch from Bitvault: {e}") }),
        ))?
        .text()
        .await
        .map_err(|e| (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: format!("Failed to read Bitvault response: {e}") }),
        ))?;

    Ok(text)
}
