use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    Json,
};
use std::sync::Arc;

use crate::api::client::OpenAiClient;
use crate::models::{
    ConfigTestRequest, ConfigTestResponse, ErrorResponse, GatedAccessRequest, StatusResponse,
};
use crate::routes::extractors::AppJson;
use crate::routes::translate::{check_authenticated, get_session_id, verify_bearer};
use crate::{AppState, SessionCredentials, SessionTier};

pub async fn get_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<StatusResponse> {
    let session_tier_str = get_session_id(&headers)
        .and_then(|sid| {
            state
                .sessions
                .read()
                .unwrap()
                .get(&sid)
                .map(|creds| match creds.tier {
                    SessionTier::Free => "free".to_string(),
                    SessionTier::Byok => "byok".to_string(),
                    SessionTier::Gated => "gated".to_string(),
                })
        })
        .or_else(|| {
            // Bearer token auth: reflect as an active gated session.
            let key = &state.config.gated_access_key;
            if !key.is_empty() && verify_bearer(&headers, key).is_ok() {
                Some("gated".to_string())
            } else {
                None
            }
        });
    let session_active = session_tier_str.is_some();

    let char_limit = match session_tier_str.as_deref() {
        Some("free") => state.config.free_tier_char_limit,
        Some("gated") => state.config.gated_char_limit,
        _ => None, // byok or no active session: unlimited
    };

    Json(StatusResponse {
        server_configured: state.config.is_configured(),
        gated_configured: state.config.is_gated_configured(),
        session_active,
        session_tier: session_tier_str,
        bitvault_configured: state.config.is_bitvault_configured(),
        tts_configured: state.config.is_tts_configured(),
        tts_languages: {
            let mut v: Vec<String> = state.config.tts_voice_map.keys().cloned().collect();
            v.sort();
            v
        },
        tts_hostname: state.config.tts_hostname(),
        tts_model: {
            let models: std::collections::BTreeSet<String> = state
                .config
                .tts_voice_map
                .values()
                .map(|e| e.model.clone())
                .collect();
            models.into_iter().next()
        },
        char_limit,
        git_commit: env!("GIT_COMMIT_SHORT"),
    })
}

pub fn make_session_cookie(sid: &str) -> String {
    let secure = std::env::var("COOKIE_SECURE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if secure {
        format!("sid={sid}; Path=/; SameSite=Strict; HttpOnly; Secure")
    } else {
        format!("sid={sid}; Path=/; SameSite=Strict; HttpOnly")
    }
}

/// Validate an endpoint+key pair without creating a session. Used when a gated user wants to
/// overlay their own translation endpoint without replacing the gated session cookie.
/// Requires an authenticated session or valid Bearer token to prevent unauthenticated SSRF.
pub async fn post_config_check(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AppJson(req): AppJson<ConfigTestRequest>,
) -> Result<Json<ConfigTestResponse>, (StatusCode, Json<ErrorResponse>)> {
    check_authenticated(&state, &headers)?;

    if req.endpoint.is_empty() || req.api_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "endpoint and api_key are required".into(),
            }),
        ));
    }

    let client = OpenAiClient::with_credentials(&req.endpoint, &req.api_key);
    let response = client
        .http
        .get(client.models_url())
        .bearer_auth(&client.api_key)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("Cannot reach endpoint: {e}"),
                }),
            )
        })?;

    let status = response.status();
    if status.is_success() {
        Ok(Json(ConfigTestResponse {
            ok: true,
            message: "Connection successful".into(),
        }))
    } else if status.as_u16() == 401 || status.as_u16() == 403 {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid API key".into(),
            }),
        ))
    } else {
        let body = response.text().await.unwrap_or_default();
        Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: format!("Endpoint returned {status}: {body}"),
            }),
        ))
    }
}

pub async fn post_config_test(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<ConfigTestRequest>,
) -> Result<(HeaderMap, Json<ConfigTestResponse>), (StatusCode, Json<ErrorResponse>)> {
    if req.endpoint.is_empty() || req.api_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "endpoint and api_key are required".into(),
            }),
        ));
    }

    let client = OpenAiClient::with_credentials(&req.endpoint, &req.api_key);

    // Test by calling GET /v1/models — standard OpenAI-compatible endpoint
    let response = client
        .http
        .get(client.models_url())
        .bearer_auth(&client.api_key)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("Cannot reach endpoint: {e}"),
                }),
            )
        })?;

    let status = response.status();

    if status.is_success() {
        let sid = uuid::Uuid::new_v4().to_string();
        {
            let mut sessions = state.sessions.write().unwrap();
            // Evict an arbitrary entry if the store is full.
            if sessions.len() >= 1000 {
                if let Some(old_key) = sessions.keys().next().cloned() {
                    sessions.remove(&old_key);
                }
            }
            sessions.insert(
                sid.clone(),
                SessionCredentials {
                    endpoint: req.endpoint.clone(),
                    api_key: req.api_key.clone(),
                    tier: SessionTier::Byok,
                },
            );
        }
        let mut resp_headers = HeaderMap::new();
        // Session cookie: no Max-Age so it expires when the browser session ends.
        resp_headers.insert(
            header::SET_COOKIE,
            make_session_cookie(&sid).parse().unwrap(),
        );
        Ok((
            resp_headers,
            Json(ConfigTestResponse {
                ok: true,
                message: "Connection successful".into(),
            }),
        ))
    } else if status.as_u16() == 401 || status.as_u16() == 403 {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid API key".into(),
            }),
        ))
    } else {
        let body = response.text().await.unwrap_or_default();
        Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: format!("Endpoint returned {status}: {body}"),
            }),
        ))
    }
}

pub async fn post_gated_access(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<GatedAccessRequest>,
) -> Result<(HeaderMap, Json<ConfigTestResponse>), (StatusCode, Json<ErrorResponse>)> {
    if !state.config.is_gated_configured() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Gated tier is not configured on this server".into(),
            }),
        ));
    }

    use subtle::ConstantTimeEq;
    if req
        .access_key
        .as_bytes()
        .ct_eq(state.config.gated_access_key.as_bytes())
        .unwrap_u8()
        == 0
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid access key".into(),
            }),
        ));
    }

    // Verify upstream before issuing session (short timeout — this is a connectivity check).
    if let Some(ref client) = state.gated_client {
        let response = client
            .http
            .get(client.models_url())
            .bearer_auth(&client.api_key)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorResponse {
                        error: format!("Cannot reach gated upstream: {e}"),
                    }),
                )
            })?;

        if !response.status().is_success() {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("Gated upstream returned status: {}", response.status()),
                }),
            ));
        }
    }

    let sid = uuid::Uuid::new_v4().to_string();
    {
        let mut sessions = state.sessions.write().unwrap();
        // Evict an arbitrary entry if the store is full.
        if sessions.len() >= 1000 {
            if let Some(old_key) = sessions.keys().next().cloned() {
                sessions.remove(&old_key);
            }
        }
        sessions.insert(
            sid.clone(),
            SessionCredentials {
                endpoint: state.config.gated_api_base_url.clone(),
                api_key: state.config.gated_api_key.clone(),
                tier: SessionTier::Gated,
            },
        );
    }

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::SET_COOKIE,
        make_session_cookie(&sid).parse().unwrap(),
    );
    Ok((
        resp_headers,
        Json(ConfigTestResponse {
            ok: true,
            message: "Access granted".into(),
        }),
    ))
}
