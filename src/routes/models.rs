use axum::{extract::State, Json};
use std::sync::Arc;

use crate::api::chunker::context_size_from_model_id;
use crate::models::{ModelInfo, ModelsResponse};
use crate::AppState;

pub async fn get_models(
    State(state): State<Arc<AppState>>,
) -> Json<ModelsResponse> {
    let mut translation_ids = state.config.translation_models.clone();
    let mut transcription_ids = state.config.whisper_models.clone();

    // If no models defined in config, try to fetch from API
    if translation_ids.is_empty() && transcription_ids.is_empty() && state.config.is_configured() {
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
