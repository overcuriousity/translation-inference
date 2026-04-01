use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use url::Url;

const MAX_PROXY_BYTES: usize = 512 * 1024; // 512 KB

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
        StatusCode::BAD_GATEWAY,
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

    let allowed = Url::parse(bitvault_url).map_err(|_| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { error: "Bitvault URL is misconfigured".into() }),
    ))?;
    let requested = Url::parse(&q.url).map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse { error: "Invalid URL".into() }),
    ))?;
    if requested.scheme() != allowed.scheme()
        || requested.host() != allowed.host()
        || requested.port_or_known_default() != allowed.port_or_known_default()
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse { error: "URL not allowed — must point to the configured Bitvault instance".into() }),
        ));
    }

    let resp = state.client.http
        .get(&q.url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: format!("Failed to fetch from Bitvault: {e}") }),
        ))?;

    if !resp.status().is_success() {
        let upstream_status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: format!("Bitvault responded with {upstream_status}: {body}") }),
        ));
    }

    if resp.content_length().map_or(false, |n| n > MAX_PROXY_BYTES as u64) {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: "Bitvault response exceeds size limit".into() }),
        ));
    }

    let bytes = resp.bytes().await.map_err(|e| (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse { error: format!("Failed to read Bitvault response: {e}") }),
    ))?;

    if bytes.len() > MAX_PROXY_BYTES {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse { error: "Bitvault response exceeds size limit".into() }),
        ));
    }

    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(text)
}
