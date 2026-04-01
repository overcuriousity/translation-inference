use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use crate::api::client::OpenAiClient;
use crate::models::{ConfigTestRequest, ConfigTestResponse, ErrorResponse, StatusResponse};
use crate::AppState;

pub async fn get_status(
    State(state): State<Arc<AppState>>,
) -> Json<StatusResponse> {
    Json(StatusResponse {
        server_configured: state.config.is_configured(),
    })
}

pub async fn post_config_test(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ConfigTestRequest>,
) -> Result<Json<ConfigTestResponse>, (StatusCode, Json<ErrorResponse>)> {
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
        Ok(Json(ConfigTestResponse {
            ok: true,
            message: "Connection successful".into(),
        }))
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
