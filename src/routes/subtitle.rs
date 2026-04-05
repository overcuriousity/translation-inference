use axum::{extract::{Multipart, State}, http::{HeaderMap, StatusCode}, Json};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use std::sync::Arc;

use crate::api::{chat::translate_single, chunker::TranslationConfig};
use crate::models::ErrorResponse;
use crate::routes::translate::resolve_client;
use crate::subtitle::{parse_srt, parse_vtt, render_srt, render_vtt};
use crate::AppState;

const MAX_FILE_BYTES: usize = 50 * 1024 * 1024; // 50 MB

/// Separator used between cue texts when batching translation.
const SEP: &str = "\n§§§\n";

#[derive(serde::Serialize)]
pub struct SubtitleResponse {
    pub filename: String,
    pub data: String,  // base64
    pub mime: String,
}

pub async fn post_translate_subtitle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<SubtitleResponse>, (StatusCode, Json<ErrorResponse>)> {
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

    let bytes = file_bytes.ok_or_else(|| err(StatusCode::BAD_REQUEST, "No file provided".into()))?;

    // Detect format from filename extension.
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

    let mut cues = if ext == "srt" {
        parse_srt(&text)
    } else {
        parse_vtt(&text)
    };

    if cues.is_empty() {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "No subtitle cues found in file".into()));
    }

    let client = resolve_client(&state, endpoint.as_deref(), api_key.as_deref(), &headers)?;
    let model_str = model.as_deref().unwrap_or(&state.config.translation_model);
    let config = TranslationConfig::from(&state.config);

    // Join all cue texts with separator and translate in one request.
    let joined: String = cues.iter().map(|c| c.lines.join("\n")).collect::<Vec<_>>().join(SEP);

    let translated = translate_single(&client, model_str, &source_lang, &target_lang, &joined, None, &config)
        .await
        .map_err(|e| {
            tracing::error!("Subtitle translation error: {e:#}");
            err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // Split translated text back on the separator.
    let translated_parts: Vec<&str> = translated.split("§§§").collect();

    // Write translations back into cues.
    for (i, cue) in cues.iter_mut().enumerate() {
        if let Some(part) = translated_parts.get(i) {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                cue.lines = trimmed.lines().map(str::to_string).collect();
            }
        }
    }

    // Render back to original format.
    let output = if ext == "srt" {
        render_srt(&cues)
    } else {
        render_vtt(&cues)
    };

    // Build output filename: insert "_translated" before extension.
    let stem = std::path::Path::new(&original_filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("subtitle");
    let out_filename = format!("{stem}_translated.{ext}");
    let data = B64.encode(output.as_bytes());

    Ok(Json(SubtitleResponse {
        filename: out_filename,
        data,
        mime: "text/plain".into(),
    }))
}

fn err(status: StatusCode, msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg }))
}
