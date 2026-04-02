use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use std::sync::Arc;

use url::Url;

use crate::api::client::OpenAiClient;
use crate::models::{ErrorResponse, TtsRequest};
use crate::routes::translate::check_authenticated;
use crate::AppState;

pub async fn post_tts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<TtsRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    check_authenticated(&state, &headers)?;

    let client = resolve_tts_client(&state, req.tts_endpoint.as_deref(), req.tts_api_key.as_deref())?;

    if req.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "text is required".into() }),
        ));
    }

    let chunks = split_into_chunks(&req.text, state.config.tts_chunk_size);
    let mut audio_bytes: Vec<u8> = Vec::new();
    let tts_model = state.config.tts_model.clone();
    let tts_voice = state.config.tts_voice.clone();

    for chunk in chunks {
        let payload = serde_json::json!({
            "model": tts_model,
            "input": chunk,
            "voice": tts_voice,
            "response_format": "mp3",
        });

        let url = format!("{}/v1/audio/speech", client.base_url.trim_end_matches('/'));

        let resp = client
            .http
            .post(&url)
            .bearer_auth(&client.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("TTS request failed: {e:#}");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorResponse { error: format!("TTS request failed: {e}") }),
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("TTS endpoint returned {status}: {body}");
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("TTS upstream error: {status}"),
                }),
            ));
        }

        let bytes = resp.bytes().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to read TTS response: {e}") }),
            )
        })?;

        audio_bytes.extend_from_slice(&bytes);
    }

    let content_length = audio_bytes.len().to_string();
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CONTENT_LENGTH, content_length)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(audio_bytes))
        .unwrap();

    Ok(response)
}

/// Reject BYOK endpoints that could be used for SSRF.
/// Scheme must be http or https; in gated (multi-user) mode private/loopback
/// targets are also blocked because the server would make the request on the
/// caller's behalf.
fn validate_byok_endpoint(
    endpoint: &str,
    gated_mode: bool,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let url = Url::parse(endpoint).map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse { error: "Invalid TTS endpoint URL.".into() }),
    ))?;

    match url.scheme() {
        "http" | "https" => {}
        _ => return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "TTS endpoint must use http or https.".into() }),
        )),
    }

    if gated_mode {
        let blocked = match url.host() {
            Some(url::Host::Ipv4(addr)) => {
                addr.is_loopback() || addr.is_private() || addr.is_link_local() || addr.is_unspecified()
            }
            Some(url::Host::Ipv6(addr)) => addr.is_loopback(),
            Some(url::Host::Domain(host)) => {
                host == "localhost" || host.ends_with(".local") || host.ends_with(".internal")
            }
            None => true,
        };
        if blocked {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "TTS endpoint may not target private or loopback addresses.".into(),
                }),
            ));
        }
    }

    Ok(())
}

fn resolve_tts_client(
    state: &AppState,
    tts_endpoint: Option<&str>,
    tts_api_key: Option<&str>,
) -> Result<OpenAiClient, (StatusCode, Json<ErrorResponse>)> {
    // Per-request BYOK TTS credentials take priority.
    if let (Some(ep), Some(key)) = (tts_endpoint, tts_api_key) {
        if !ep.is_empty() && !key.is_empty() {
            let gated_mode = !state.config.gated_access_key.is_empty();
            validate_byok_endpoint(ep, gated_mode)?;
            return Ok(OpenAiClient::with_credentials(ep, key));
        }
    }
    // Fall back to server-configured TTS client.
    if let Some(ref tc) = state.tts_client {
        return Ok(tc.clone());
    }
    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse {
            error: "No TTS backend configured. Provide tts_endpoint and tts_api_key in the request or configure TTS_API_BASE_URL on the server.".into(),
        }),
    ))
}

/// Split text into chunks of at most `max_bytes` bytes, breaking at sentence
/// boundaries where possible. If `max_bytes` is `None`, the text is returned
/// as a single chunk (suitable for local models with no size limit).
fn split_into_chunks(text: &str, max_bytes: Option<usize>) -> Vec<String> {
    let Some(limit) = max_bytes else {
        return vec![text.to_string()];
    };

    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for sentence in split_sentences(text) {
        if current.len() + sentence.len() > limit && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current = String::new();
        }
        // If a single sentence exceeds the limit, hard-split it.
        if sentence.len() > limit {
            if !current.is_empty() {
                chunks.push(current.trim().to_string());
                current = String::new();
            }
            // Use char_indices so all slice boundaries are valid UTF-8.
            let mut chunk_start = 0usize;
            let mut current_len = 0usize;
            let mut last_space: Option<usize> = None;
            for (i, ch) in sentence.char_indices() {
                let ch_len = ch.len_utf8();
                if current_len + ch_len > limit {
                    // Prefer breaking at the last space; otherwise split before current char.
                    let split_at = match last_space {
                        Some(sp) if sp > chunk_start => sp,
                        _ => i,
                    };
                    let part = sentence[chunk_start..split_at].trim();
                    if !part.is_empty() {
                        chunks.push(part.to_string());
                    }
                    chunk_start = split_at;
                    current_len = sentence[chunk_start..i].len();
                    last_space = None;
                }
                current_len += ch_len;
                if ch == ' ' {
                    last_space = Some(i + ch_len);
                }
            }
            if chunk_start < sentence.len() {
                let part = sentence[chunk_start..].trim();
                if !part.is_empty() {
                    chunks.push(part.to_string());
                }
            }
        } else {
            current.push_str(&sentence);
        }
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Yield sentence-like spans from `text`, preserving trailing whitespace so
/// that rejoining produces the original string.
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        if matches!(ch, '.' | '!' | '?') || ch == '\n' {
            // Include the punctuation and any following whitespace in this span.
            let mut end = i + 1;
            while end < len && chars[end] == ' ' {
                end += 1;
            }
            let span: String = chars[start..end].iter().collect();
            sentences.push(span);
            start = end;
            i = end;
        } else {
            i += 1;
        }
    }

    if start < len {
        sentences.push(chars[start..].iter().collect());
    }

    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_LIMIT: usize = 4000;

    #[test]
    fn short_text_is_single_chunk() {
        let chunks = split_into_chunks("Hello world.", Some(TEST_LIMIT));
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn no_limit_is_single_chunk() {
        let text = "x".repeat(TEST_LIMIT * 10);
        let chunks = split_into_chunks(&text, None);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn long_text_splits_at_sentence_boundary() {
        let sentence = "This is a sentence. ";
        let text = sentence.repeat((TEST_LIMIT / sentence.len()) + 5);
        let chunks = split_into_chunks(&text, Some(TEST_LIMIT));
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= TEST_LIMIT);
        }
    }
}
