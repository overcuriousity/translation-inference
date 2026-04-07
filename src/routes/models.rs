use axum::{extract::State, http::HeaderMap, Json};
use std::sync::Arc;

use crate::api::chunker::{context_size_from_model_id, TranslationConfig};
use crate::api::client::{ModelKind, OpenAiClient};
use crate::models::{ModelInfo, ModelsResponse};
use crate::routes::translate::get_session_id;
use crate::AppState;

/// Categorize a list of model IDs into translation / transcription buckets using
/// the capability cache. Models not yet in the cache are probed on-demand and the
/// result is stored for subsequent calls.
///
/// Classification is purely behaviour-based (see `OpenAiClient::probe_model_kind`):
/// - Translation  → chat probe succeeded (text in, text out)
/// - Transcription → STT probe succeeded (audio in, text out)
/// - Tts / Unknown → excluded from all dropdowns
async fn categorize_models(
    fetched: Vec<String>,
    client: &OpenAiClient,
    cache: &std::sync::RwLock<std::collections::HashMap<(String, String), ModelKind>>,
    translation: &mut Vec<String>,
    transcription: &mut Vec<String>,
) {
    for id in fetched {
        let key = (client.base_url.clone(), id.clone());

        let cached = cache.read().unwrap().get(&key).copied();
        let kind = match cached {
            Some(k) => k,
            None => {
                let k = client.probe_model_kind(&id).await;
                tracing::info!(model = %id, kind = ?k, "model capability probe (on-demand)");
                cache.write().unwrap().insert(key, k);
                k
            }
        };

        match kind {
            ModelKind::Translation => translation.push(id),
            ModelKind::Transcription => transcription.push(id),
            ModelKind::Tts | ModelKind::Unknown => {} // excluded
        }
    }
}

pub async fn get_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ModelsResponse> {
    let mut translation_ids = Vec::new();
    let mut transcription_ids = Vec::new();

    // Free-tier sessions use the server-configured model list (same as unauthenticated),
    // not a live API fetch — the free tier has no user-supplied credentials to discover.
    let session_client: Option<OpenAiClient> = get_session_id(&headers).and_then(|sid| {
        state
            .sessions
            .read()
            .unwrap()
            .get(&sid)
            .and_then(|c| match c.tier {
                crate::SessionTier::Gated => Some(
                    state
                        .gated_client
                        .clone()
                        .unwrap_or_else(|| state.client.clone()),
                ),
                crate::SessionTier::Free => None,
                crate::SessionTier::Byok => {
                    Some(OpenAiClient::with_credentials(&c.endpoint, &c.api_key))
                }
            })
    });

    if let Some(client) = session_client {
        if let Ok(fetched) = client.fetch_models().await {
            categorize_models(
                fetched,
                &client,
                &state.model_capabilities,
                &mut translation_ids,
                &mut transcription_ids,
            )
            .await;
        }
    } else {
        translation_ids = state.config.translation_models.clone();
        transcription_ids = state.config.whisper_models.clone();

        if translation_ids.is_empty()
            && transcription_ids.is_empty()
            && state.config.is_configured()
        {
            if let Ok(fetched) = state.client.fetch_models().await {
                categorize_models(
                    fetched,
                    &state.client,
                    &state.model_capabilities,
                    &mut translation_ids,
                    &mut transcription_ids,
                )
                .await;
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
