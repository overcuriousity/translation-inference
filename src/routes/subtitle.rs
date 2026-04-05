use axum::{
    extract::{Multipart, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, Sse},
    Json,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use std::convert::Infallible;
use std::sync::Arc;

use crate::api::{chat::translate_single, chunker::TranslationConfig};
use crate::models::ErrorResponse;
use crate::routes::translate::{check_authenticated, get_char_limit, resolve_client};
use crate::subtitle::{parse_srt, parse_vtt, render_srt, render_vtt};
use crate::AppState;

// Subtitle files are plain text; 5 MB is a generous upper bound.
const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;

pub async fn post_translate_subtitle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
    // Authenticate before touching the request body to prevent unauthenticated
    // clients from forcing file I/O and memory allocation.
    check_authenticated(&state, &headers)?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename = String::from("subtitle");
    let mut source_lang = String::from("auto");
    let mut target_lang = String::from("English");
    let mut model: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut api_key: Option<String> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                original_filename = field.file_name().unwrap_or("subtitle").to_string();
                let mut bytes = Vec::new();
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?
                {
                    bytes.extend_from_slice(&chunk);
                    if bytes.len() > MAX_FILE_BYTES {
                        return Err(err(StatusCode::PAYLOAD_TOO_LARGE, "File too large".into()));
                    }
                }
                file_bytes = Some(bytes);
            }
            Some("source_lang") => {
                source_lang = field
                    .text()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            Some("target_lang") => {
                target_lang = field
                    .text()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            Some("model") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() {
                    model = Some(v);
                }
            }
            Some("endpoint") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() {
                    endpoint = Some(v);
                }
            }
            Some("api_key") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() {
                    api_key = Some(v);
                }
            }
            _ => {}
        }
    }

    let bytes = file_bytes
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "No file provided".into()))?;

    let ext = std::path::Path::new(&original_filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext != "srt" && ext != "vtt" {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Unsupported file type: .{ext}. Only .srt and .vtt are supported."),
        ));
    }

    let text = String::from_utf8(bytes)
        .map_err(|_| err(StatusCode::UNPROCESSABLE_ENTITY, "File is not valid UTF-8".into()))?;

    let (vtt_metadata, mut cues) = if ext == "srt" {
        (vec![], parse_srt(&text))
    } else {
        parse_vtt(&text)
    };

    if cues.is_empty() {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "No subtitle cues found in file".into(),
        ));
    }

    if let Some(limit) = get_char_limit(&state, &headers) {
        let total_chars: usize = cues.iter()
            .flat_map(|c| c.lines.iter())
            .map(|l| l.chars().count())
            .sum();
        if total_chars > limit {
            return Err(err(
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("Subtitle content is {total_chars} characters, which exceeds the {limit}-character limit for your access tier."),
            ));
        }
    }

    let client = resolve_client(&state, endpoint.as_deref(), api_key.as_deref(), &headers)?;
    let model_str = model
        .as_deref()
        .unwrap_or(&state.config.translation_model)
        .to_string();
    let config = TranslationConfig::from(&state.config);

    let stem = std::path::Path::new(&original_filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("subtitle")
        .to_string();
    let out_filename = format!("{stem}_translated.{ext}");

    let stream = async_stream::stream! {
        let total = cues.len();

        for i in 0..total {
            let cue_text = cues[i].lines.join("\n");

            match translate_single(
                &client,
                &model_str,
                &source_lang,
                &target_lang,
                &cue_text,
                None,
                &config,
            )
            .await
            {
                Ok(translated) => {
                    let trimmed = translated.trim().to_string();
                    if !trimmed.is_empty() {
                        cues[i].lines = trimmed.lines().map(str::to_string).collect();
                    }
                    let data = format!(r#"{{"done":{},"total":{}}}"#, i + 1, total);
                    yield Ok::<_, Infallible>(Event::default().event("progress").data(data));
                }
                Err(e) => {
                    tracing::error!("Subtitle cue translation error: {e:#}");
                    let data = format!(
                        r#"{{"error":{}}}"#,
                        serde_json::Value::String(e.to_string())
                    );
                    yield Ok::<_, Infallible>(Event::default().event("error").data(data));
                    return;
                }
            }
        }

        let output = if ext == "srt" {
            render_srt(&cues)
        } else {
            render_vtt(&vtt_metadata, &cues)
        };

        let encoded = B64.encode(output.as_bytes());
        let data = format!(
            r#"{{"filename":{},"data":{},"mime":"text/plain"}}"#,
            serde_json::Value::String(out_filename),
            serde_json::Value::String(encoded),
        );
        yield Ok::<_, Infallible>(Event::default().event("done").data(data));
    };

    Ok(Sse::new(stream))
}

fn err(status: StatusCode, msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg }))
}
