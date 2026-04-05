use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use futures::StreamExt;
use std::sync::Arc;

use crate::api::{chat, chunker::TranslationConfig, client::OpenAiClient};
use crate::models::{ErrorResponse, TranslateRequest, TranslateResponse};
use crate::AppState;

/// Extract the `sid` session cookie from request headers.
pub fn get_session_id(headers: &HeaderMap) -> Option<String> {
    let cookie_str = headers.get("cookie")?.to_str().ok()?;
    for pair in cookie_str.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        if parts.next()?.trim() == "sid" {
            return Some(parts.next()?.trim().to_string());
        }
    }
    None
}

pub async fn post_translate_stream(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TranslateRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if let Some(limit) = get_char_limit(&state, &headers) {
        let len = req.text.chars().count();
        if len > limit {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, Json(ErrorResponse {
                error: format!("Input is {len} characters, which exceeds the {limit}-character limit for your access tier."),
            })));
        }
    }

    let client = resolve_translation_client(
        &state,
        req.endpoint.as_deref(),
        req.api_key.as_deref(),
        &headers,
    )?;

    let model = req
        .model
        .as_deref()
        .unwrap_or(&state.config.translation_model)
        .to_string();

    let stream = chat::translate_stream(
        client,
        model,
        req.source_lang.unwrap_or_else(|| "auto".to_string()),
        req.target_lang,
        req.text,
        req.context,
        TranslationConfig::from(&state.config),
    );

    let byte_stream = stream.map(|result| {
        match result {
            Ok(text) => Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(text)),
            Err(e) => {
                tracing::error!("Translation stream error: {e:#}");
                // HTTP 200 is already committed; signal the error to the client via
                // a null-byte sentinel that normal translation output can never contain.
                Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(format!("\x00ERR:{e:#}")))
            }
        }
    });

    let body = Body::from_stream(byte_stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::TRANSFER_ENCODING, "chunked")
        .body(body)
        .unwrap();

    Ok(response)
}

pub async fn post_translate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TranslateRequest>,
) -> Result<Json<TranslateResponse>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(limit) = get_char_limit(&state, &headers) {
        let len = req.text.chars().count();
        if len > limit {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, Json(ErrorResponse {
                error: format!("Input is {len} characters, which exceeds the {limit}-character limit for your access tier."),
            })));
        }
    }

    if req.text.trim().is_empty() {
        return Ok(Json(TranslateResponse {
            translated_text: String::new(),
            chunks_total: 0,
            chunks_completed: 0,
        }));
    }

    let client = resolve_translation_client(
        &state,
        req.endpoint.as_deref(),
        req.api_key.as_deref(),
        &headers,
    )?;

    let model = req
        .model
        .as_deref()
        .unwrap_or(&state.config.translation_model);

    let translation_config = TranslationConfig::from(&state.config);
    let source_lang = req.source_lang.as_deref().unwrap_or("auto");
    match chat::translate(
        &client,
        model,
        source_lang,
        &req.target_lang,
        &req.text,
        req.context.as_deref(),
        &translation_config,
    )
    .await
    {
        Ok((translated_text, chunks_total, chunks_completed)) => Ok(Json(TranslateResponse {
            translated_text,
            chunks_total,
            chunks_completed,
        })),
        Err(e) => {
            tracing::error!("Translation error: {e:#}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

/// Validate the Bearer token against `access_key` using a constant-time compare.
/// Returns `Ok(())` on match, or an UNAUTHORIZED error.
fn verify_bearer(
    headers: &HeaderMap,
    access_key: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    use subtle::ConstantTimeEq;
    if provided.as_bytes().ct_eq(access_key.as_bytes()).unwrap_u8() == 0 {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid or missing access key.".into(),
            }),
        ));
    }
    Ok(())
}

/// Verify the request is authenticated (session cookie or Bearer token).
/// Returns `Ok(())` if authenticated, or the appropriate error response.
pub fn check_authenticated(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Some(sid) = get_session_id(headers) {
        if state.sessions.read().unwrap().contains_key(&sid) {
            return Ok(());
        }
    }

    let access_key = &state.config.gated_access_key;
    if access_key.is_empty() {
        // Personal/local mode: no gated key configured.
        // Allow access when the server itself is configured (mirrors resolve_client behaviour).
        if state.config.is_configured() || state.config.is_tts_configured() {
            return Ok(());
        }
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Direct API access is disabled. Use the web interface.".into(),
            }),
        ));
    }

    // Gated mode: Bearer token required for all direct API requests.
    verify_bearer(headers, access_key)
}

/// Returns the character limit for the session making this request, or `None` for unlimited.
pub fn get_char_limit(state: &AppState, headers: &HeaderMap) -> Option<usize> {
    if let Some(sid) = get_session_id(headers) {
        if let Some(creds) = state.sessions.read().unwrap().get(&sid) {
            return match creds.tier {
                crate::SessionTier::Free => state.config.free_tier_char_limit,
                crate::SessionTier::Gated => state.config.gated_char_limit,
                crate::SessionTier::Byok => None,
            };
        }
    }
    // Bearer-authenticated direct API access in gated mode → apply gated limit.
    if !state.config.gated_access_key.is_empty()
        && verify_bearer(headers, &state.config.gated_access_key).is_ok()
    {
        return state.config.gated_char_limit;
    }
    // Personal/local mode (no session, no gated key): treat as unlimited.
    None
}

pub fn resolve_client(
    state: &AppState,
    endpoint: Option<&str>,
    api_key: Option<&str>,
    headers: &HeaderMap,
) -> Result<OpenAiClient, (StatusCode, Json<ErrorResponse>)> {
    // Session cookie → web interface path, dispatch by tier.
    if let Some(sid) = get_session_id(headers) {
        if let Some(creds) = state.sessions.read().unwrap().get(&sid) {
            return match creds.tier {
                crate::SessionTier::Free => Ok(state.client.clone()),
                crate::SessionTier::Gated => {
                    if let Some(ref gc) = state.gated_client {
                        Ok(gc.clone())
                    } else {
                        Ok(state.client.clone())
                    }
                }
                crate::SessionTier::Byok => Ok(OpenAiClient::with_credentials(
                    &creds.endpoint,
                    &creds.api_key,
                )),
            };
        }
    }

    // No session cookie → direct API access.
    // If GATED_ACCESS_KEY is not configured but the server itself is configured,
    // allow unauthenticated access (personal/local deployment mode).
    let access_key = &state.config.gated_access_key;
    if access_key.is_empty() {
        if state.config.is_configured() {
            return Ok(state.client.clone());
        }
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "No API credentials configured. Please set up your endpoint via the web interface.".into(),
            }),
        ));
    }

    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if provided.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Authentication required. Provide a Bearer token or use the web interface."
                    .into(),
            }),
        ));
    }

    // Bearer token provided → must match the gated access key.
    use subtle::ConstantTimeEq;
    if provided.as_bytes().ct_eq(access_key.as_bytes()).unwrap_u8() == 0 {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid access key.".into(),
            }),
        ));
    }

    // Authenticated: use per-request BYOK credentials if provided,
    // then gated client, then server-level fallback.
    if let (Some(ep), Some(key)) = (endpoint, api_key) {
        if !ep.is_empty() && !key.is_empty() {
            return Ok(OpenAiClient::with_credentials(ep, key));
        }
    }
    if let Some(ref gc) = state.gated_client {
        return Ok(gc.clone());
    }
    if state.config.is_configured() {
        return Ok(state.client.clone());
    }
    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "No API credentials configured. Provide endpoint and api_key in the request."
                .into(),
        }),
    ))
}

/// Like `resolve_client` but also honours per-request `endpoint`/`api_key` for gated sessions.
/// Used by translation routes so that a gated user can overlay their own LLM endpoint without
/// replacing their session. STT routes deliberately do NOT use this — they have their own
/// `stt_endpoint`/`stt_api_key` overlay mechanism.
pub fn resolve_translation_client(
    state: &AppState,
    endpoint: Option<&str>,
    api_key: Option<&str>,
    headers: &HeaderMap,
) -> Result<OpenAiClient, (StatusCode, Json<ErrorResponse>)> {
    if let Some(sid) = get_session_id(headers) {
        if let Some(creds) = state.sessions.read().unwrap().get(&sid) {
            if matches!(creds.tier, crate::SessionTier::Gated) {
                // Gated session: allow a per-request BYOK translation override.
                if let (Some(ep), Some(key)) = (endpoint, api_key) {
                    if !ep.is_empty() && !key.is_empty() {
                        return Ok(OpenAiClient::with_credentials(ep, key));
                    }
                }
            }
        }
    }
    // All other cases fall through to the standard resolver.
    resolve_client(state, endpoint, api_key, headers)
}
