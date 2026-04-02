use axum::{extract::State, http::HeaderMap, Json};
use std::sync::Arc;

use crate::api::chunker::context_size_from_model_id;
use crate::api::client::OpenAiClient;
use crate::models::{ModelInfo, ModelsResponse};
use crate::routes::translate::get_session_id;
use crate::AppState;

pub async fn get_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ModelsResponse> {
    let mut translation_ids = state.config.translation_models.clone();
    let mut transcription_ids = state.config.whisper_models.clone();

    // If no models defined in config, try to fetch from any available client.
    // Session takes priority so users get models from their active tier (gated or BYOK).
    if translation_ids.is_empty() && transcription_ids.is_empty() {
        let client_opt: Option<OpenAiClient> = if let Some(sid) = get_session_id(&headers) {
            state.sessions.read().unwrap()
                .get(&sid)
                .map(|c| OpenAiClient::with_credentials(&c.endpoint, &c.api_key))
        } else if state.config.is_configured() {
            Some(state.client.clone())
        } else {
            None
        };

        if let Some(client) = client_opt {
            if let Ok(fetched) = client.fetch_models().await {
                for id in fetched {
                    if id.to_lowercase().contains("whisper") {
                        transcription_ids.push(id);
                    } else {
                        translation_ids.push(id);
                    }
                }
            }
        }
    }

    // Fallbacks if still empty
    if translation_ids.is_empty() {
        translation_ids.push(state.config.translation_model.clone());
    }
    if transcription_ids.is_empty() {
        transcription_ids.push(state.config.whisper_model.clone());
    }

    let translation_models = translation_ids
        .into_iter()
        .map(|id| ModelInfo {
            name: id.clone(),
            context_size: context_size_from_model_id(&id),
            id,
        })
        .collect();

    let transcription_models = transcription_ids
        .into_iter()
        .map(|id| ModelInfo {
            name: id.clone(),
            context_size: context_size_from_model_id(&id),
            id,
        })
        .collect();

    Json(ModelsResponse {
        translation_models,
        transcription_models,
    })
}
