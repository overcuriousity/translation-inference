use axum::{extract::State, http::{header, HeaderMap, StatusCode}, Json};
use std::sync::Arc;

use crate::api::client::OpenAiClient;
use crate::models::{ConfigTestRequest, ConfigTestResponse, ErrorResponse, StatusResponse};
use crate::routes::translate::get_session_id;
use crate::{AppState, SessionCredentials};

pub async fn get_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<StatusResponse> {
    let session_active = get_session_id(&headers)
        .map(|sid| state.sessions.read().unwrap().contains_key(&sid))
        .unwrap_or(false);
    Json(StatusResponse {
        server_configured: state.config.is_configured(),
        session_active,
    })
}

pub async fn post_config_test(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigTestRequest>,
) -> Result<(HeaderMap, Json<ConfigTestResponse>), (StatusCode, Json<ErrorResponse>)> {
    if req.endpoint.is_empty() || req.api_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "endpoint and api_key are required".into() }),
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
        state.sessions.write().unwrap().insert(
            sid.clone(),
            SessionCredentials { endpoint: req.endpoint.clone(), api_key: req.api_key.clone() },
        );
        let mut resp_headers = HeaderMap::new();
        // Session cookie: no Max-Age so it expires when the browser session ends.
        resp_headers.insert(
            header::SET_COOKIE,
            format!("sid={sid}; Path=/; SameSite=Strict; HttpOnly")
                .parse()
                .unwrap(),
        );
        Ok((resp_headers, Json(ConfigTestResponse {
            ok: true,
            message: "Connection successful".into(),
        })))
    } else if status.as_u16() == 401 || status.as_u16() == 403 {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse { error: "Invalid API key".into() }),
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
