use axum::{extract::State, http::HeaderMap, Json};
use std::sync::Arc;

use crate::api::chunker::{context_size_from_model_id, TranslationConfig};
use crate::api::client::{ModelKind, OpenAiClient};
use crate::models::{ModelInfo, ModelsResponse};
use crate::routes::translate::get_session_id;
use crate::AppState;

/// Categorize a list of model IDs into translation / transcription buckets.
///
/// Classification is behaviour-based via `OpenAiClient::probe_model_kind`:
///   - Translation  → chat probe succeeded
///   - Transcription → STT probe succeeded
///   - Tts / Unknown → excluded from all dropdowns
///
/// `cache` should be `Some(&state.model_capabilities)` for server-managed
/// sessions (gated) so that probe results persist across requests.
/// Pass `None` for BYOK sessions — results are used only for this request
/// and never written to the shared cache. This means BYOK re-probes on every
/// `/api/models` call; this is intentional to prevent cross-user cache
/// pollution (different users may present different BYOK endpoints/keys).
///
/// On a transient probe failure (`probe_model_kind` returns `None`) the model
/// is assumed to be a Translation model so it is not silently hidden from the
/// user; the next `/api/models` call will probe it again.
async fn categorize_models(
    fetched: Vec<String>,
    client: &OpenAiClient,
    cache: Option<&std::sync::RwLock<std::collections::HashMap<(String, String), ModelKind>>>,
    tts_voice: Option<&str>,
    translation: &mut Vec<String>,
    transcription: &mut Vec<String>,
) {
    for id in fetched {
        // Check the shared cache first (only populated for server-managed clients).
        let cached = cache.and_then(|c| {
            c.read()
                .unwrap()
                .get(&(client.base_url.clone(), id.clone()))
                .copied()
        });

        let kind = match cached {
            Some(k) => k,
            None => {
                match client.probe_model_kind(&id, tts_voice).await {
                    Some(k) => {
                        // Only write back for server-managed sessions (cache is Some).
                        if let Some(c) = cache {
                            c.write()
                                .unwrap()
                                .insert((client.base_url.clone(), id.clone()), k);
                        }
                        tracing::info!(model = %id, kind = ?k, "model capability probe (on-demand)");
                        k
                    }
                    None => {
                        // Transient failure — do not cache. Assume Translation so the
                        // model is not silently hidden; it will be re-probed next time.
                        tracing::warn!(model = %id, "probe inconclusive (transient); assuming Translation");
                        ModelKind::Translation
                    }
                }
            }
        };

        match kind {
            ModelKind::Translation => translation.push(id),
            ModelKind::Transcription => transcription.push(id),
            ModelKind::Tts | ModelKind::Unknown => {} // excluded from all dropdowns
        }
    }
}

pub async fn get_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ModelsResponse> {
    let mut translation_ids = Vec::new();
    let mut transcription_ids = Vec::new();

    // Use a configured TTS voice for the probe so it is as realistic as possible.
    let tts_voice: Option<String> = state
        .config
        .tts_voice_map
        .values()
        .next()
        .map(|e| e.voice.clone());
    let tts_voice_ref = tts_voice.as_deref();

    // Free-tier sessions use the server-configured model list (same as unauthenticated),
    // not a live API fetch — the free tier has no user-supplied credentials to discover.
    //
    // Gated sessions fetch live and use the shared capability cache (populated at startup).
    // BYOK sessions fetch live but probe without writing to the shared cache — results
    // are scoped to this request only, avoiding cross-user cache pollution.
    enum SessionKind {
        UseCache(OpenAiClient), // gated: probe results persist to AppState cache
        NoCache(OpenAiClient),  // byok: probe inline, never write to shared cache
    }

    let session = get_session_id(&headers).and_then(|sid| {
        state
            .sessions
            .read()
            .unwrap()
            .get(&sid)
            .and_then(|c| match c.tier {
                crate::SessionTier::Gated => {
                    let client = state
                        .gated_client
                        .clone()
                        .unwrap_or_else(|| state.client.clone());
                    Some(SessionKind::UseCache(client))
                }
                crate::SessionTier::Free => None,
                crate::SessionTier::Byok => Some(SessionKind::NoCache(
                    OpenAiClient::with_credentials(&c.endpoint, &c.api_key),
                )),
            })
    });

    match session {
        Some(SessionKind::UseCache(client)) => {
            if let Ok(fetched) = client.fetch_models().await {
                categorize_models(
                    fetched,
                    &client,
                    Some(&state.model_capabilities),
                    tts_voice_ref,
                    &mut translation_ids,
                    &mut transcription_ids,
                )
                .await;
            }
        }
        Some(SessionKind::NoCache(client)) => {
            if let Ok(fetched) = client.fetch_models().await {
                categorize_models(
                    fetched,
                    &client,
                    None, // BYOK: no shared cache
                    tts_voice_ref,
                    &mut translation_ids,
                    &mut transcription_ids,
                )
                .await;
            }
        }
        None => {
            // Free tier or unauthenticated: use the server-configured lists.
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
                        Some(&state.model_capabilities),
                        tts_voice_ref,
                        &mut translation_ids,
                        &mut transcription_ids,
                    )
                    .await;
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
