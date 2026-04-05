use axum::{extract::State, http::HeaderMap, Json};
use std::sync::Arc;

use crate::api::chunker::{context_size_from_model_id, TranslationConfig};
use crate::api::client::OpenAiClient;
use crate::models::{ModelInfo, ModelsResponse};
use crate::routes::translate::get_session_id;
use crate::AppState;

pub async fn get_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ModelsResponse> {
    let mut translation_ids = Vec::new();
    let mut transcription_ids = Vec::new();

    let session_client: Option<OpenAiClient> = get_session_id(&headers).and_then(|sid| {
        state
            .sessions
            .read()
            .unwrap()
            .get(&sid)
            .map(|c| match c.tier {
                crate::SessionTier::Gated => state
                    .gated_client
                    .clone()
                    .unwrap_or_else(|| state.client.clone()),
                crate::SessionTier::Free => state.client.clone(),
                crate::SessionTier::Byok => OpenAiClient::with_credentials(&c.endpoint, &c.api_key),
            })
    });

    if let Some(client) = session_client {
        if let Ok(fetched) = client.fetch_models().await {
            for id in fetched {
                if id.to_lowercase().contains("whisper") {
                    transcription_ids.push(id);
                } else {
                    translation_ids.push(id);
                }
            }
        }
    } else {
        translation_ids = state.config.translation_models.clone();
        transcription_ids = state.config.whisper_models.clone();

        if translation_ids.is_empty() && transcription_ids.is_empty() {
            if state.config.is_configured() {
                if let Ok(fetched) = state.client.fetch_models().await {
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
    }

    // Fallbacks if still empty
    if translation_ids.is_empty() {
        translation_ids.push(state.config.translation_model.clone());
    }
    if transcription_ids.is_empty() {
        transcription_ids.push(state.config.whisper_model.clone());
    }

    let cfg = TranslationConfig::from(&state.config);

    let translation_models = translation_ids
        .into_iter()
        .map(|id| ModelInfo {
            name: id.clone(),
            context_size: context_size_from_model_id(&id, &cfg),
            id,
        })
        .collect();

    let transcription_models = transcription_ids
        .into_iter()
        .map(|id| ModelInfo {
            name: id.clone(),
            context_size: context_size_from_model_id(&id, &cfg),
            id,
        })
        .collect();

    Json(ModelsResponse {
        translation_models,
        transcription_models,
    })
}
