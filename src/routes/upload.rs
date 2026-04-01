use axum::{extract::{Multipart, State}, http::{HeaderMap, StatusCode}, Json};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use std::path::Path;
use std::sync::Arc;

use crate::api::whisper;
use crate::document;
use crate::models::{ErrorResponse, UploadResponse, UploadResult};
use crate::routes::translate::resolve_client;
use crate::AppState;

const MAX_FILE_BYTES: usize = 100 * 1024 * 1024; // 100 MB

pub async fn post_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut uploaded_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut source_lang = String::from("auto");
    let mut target_lang = String::from("English");
    let mut model: Option<String> = None;
    let mut whisper_model: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut api_key: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                let filename = field.file_name().unwrap_or("upload").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if data.len() > MAX_FILE_BYTES {
                    return Err(err(StatusCode::PAYLOAD_TOO_LARGE, format!("{filename}: file too large")));
                }
                uploaded_files.push((filename, data.to_vec()));
            }
            Some("source_lang") => {
                source_lang = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            Some("target_lang") => {
                target_lang = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
            }
            Some("model") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { model = Some(v); }
            }
            Some("whisper_model") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { whisper_model = Some(v); }
            }
            Some("endpoint") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { endpoint = Some(v); }
            }
            Some("api_key") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { api_key = Some(v); }
            }
            _ => {}
        }
    }

    if uploaded_files.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "No files provided".into()));
    }

    let client = resolve_client(&state, endpoint.as_deref(), api_key.as_deref(), &headers)?;
    let model_str = model.as_deref().unwrap_or(&state.config.translation_model);
    let whisper_model_str = whisper_model.as_deref().unwrap_or(&state.config.whisper_model);

    let mut results: Vec<UploadResult> = Vec::new();

    for (filename, bytes) in uploaded_files {
        let ext = Path::new(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let stem = Path::new(&filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");

        if is_audio_video(&ext) {
            let text = whisper::transcribe(&client, whisper_model_str, bytes, &filename)
                .await
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{filename}: {e:#}")))?;
            results.push(UploadResult::Text { filename, text });
        } else {
            match ext.as_str() {
                "docx" => {
                    let out = document::translate_docx(&bytes, &client, model_str, &source_lang, &target_lang)
                        .await
                        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{filename}: {e:#}")))?;
                    results.push(UploadResult::Document {
                        filename: format!("{stem}_translated.docx"),
                        data: B64.encode(&out),
                        mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
                    });
                }
                "odt" => {
                    let out = document::translate_odt(&bytes, &client, model_str, &source_lang, &target_lang)
                        .await
                        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{filename}: {e:#}")))?;
                    results.push(UploadResult::Document {
                        filename: format!("{stem}_translated.odt"),
                        data: B64.encode(&out),
                        mime: "application/vnd.oasis.opendocument.text".into(),
                    });
                }
                "pdf" => {
                    let out = document::translate_pdf(&bytes, &client, model_str, &source_lang, &target_lang)
                        .await
                        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{filename}: {e:#}")))?;
                    results.push(UploadResult::Document {
                        filename: format!("{stem}_translated.pdf"),
                        data: B64.encode(&out),
                        mime: "application/pdf".into(),
                    });
                }
                _ => {
                    return Err(err(StatusCode::BAD_REQUEST, format!("Unsupported file type: .{ext}")));
                }
            }
        }
    }

    Ok(Json(UploadResponse { results }))
}

fn is_audio_video(ext: &str) -> bool {
    matches!(
        ext,
        "mp3"
            | "wav"
            | "m4a"
            | "ogg"
            | "flac"
            | "aac"
            | "mp4"
            | "mkv"
            | "avi"
            | "mov"
            | "webm"
    )
}

fn err(status: StatusCode, msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg }))
}
