use axum::{extract::{Multipart, State}, http::{HeaderMap, StatusCode}, Json};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use crate::api::whisper;
use crate::api::client::OpenAiClient;
use crate::models::{ErrorResponse, UploadResponse, UploadResult};
use crate::routes::translate::{check_authenticated, resolve_client};
use crate::AppState;

const MAX_FILE_BYTES: usize = 100 * 1024 * 1024; // 100 MB

struct UploadedFile {
    filename: String,
    tmp: NamedTempFile,
}

pub async fn post_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Reject unauthenticated callers in gated mode before reading any multipart data.
    check_authenticated(&state, &headers)?;

    let mut uploaded_files: Vec<UploadedFile> = Vec::new();
    let mut whisper_model: Option<String> = None;
    let mut language: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut api_key: Option<String> = None;
    let mut stt_endpoint: Option<String> = None;
    let mut stt_api_key: Option<String> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                let filename = field.file_name().unwrap_or("upload").to_string();
                let ext = std::path::Path::new(&filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !is_audio_video(&ext) {
                    return Err(err(
                        StatusCode::BAD_REQUEST,
                        format!("Unsupported file type: .{ext}"),
                    ));
                }

                let tmp = tempfile::Builder::new()
                    .suffix(&format!(".{ext}"))
                    .tempfile()
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                let mut tmp_file = tokio::fs::File::from(
                    tmp.reopen().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
                );

                let mut size = 0;
                while let Some(chunk) = field.chunk().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))? {
                    size += chunk.len();
                    if size > MAX_FILE_BYTES {
                        return Err(err(StatusCode::PAYLOAD_TOO_LARGE, format!("{filename}: file too large")));
                    }
                    tmp_file.write_all(&chunk).await.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                }
                tmp_file.flush().await.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                uploaded_files.push(UploadedFile { filename, tmp });
            }
            Some("whisper_model") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { whisper_model = Some(v); }
            }
            Some("language") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { language = Some(v); }
            }
            Some("endpoint") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { endpoint = Some(v); }
            }
            Some("api_key") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { api_key = Some(v); }
            }
            Some("stt_endpoint") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { stt_endpoint = Some(v); }
            }
            Some("stt_api_key") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { stt_api_key = Some(v); }
            }
            _ => {}
        }
    }

    if uploaded_files.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "No files provided".into()));
    }

    // Authentication was already enforced above; use stt_endpoint/stt_api_key directly
    // if provided, otherwise fall back to the standard client resolver.
    let client = if let (Some(ep), Some(key)) = (stt_endpoint.as_deref(), stt_api_key.as_deref()) {
        if !ep.is_empty() && !key.is_empty() {
            OpenAiClient::with_credentials(ep, key)
        } else {
            resolve_client(&state, endpoint.as_deref(), api_key.as_deref(), &headers)?
        }
    } else {
        resolve_client(&state, endpoint.as_deref(), api_key.as_deref(), &headers)?
    };
    let whisper_model_str = whisper_model.as_deref().unwrap_or(&state.config.whisper_model);

    let mut results: Vec<UploadResult> = Vec::new();

    for file in uploaded_files {
        let wav_tmp = whisper::extract_audio_from_video(file.tmp.path())
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{}: {e:#}", file.filename)))?;
        let final_path = wav_tmp.path().to_path_buf();
        let _wav_tmp = wav_tmp;

        let text = whisper::transcribe(&client, whisper_model_str, language.as_deref(), &final_path, "extracted.wav")
            .await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{}: {e:#}", file.filename)))?;
        results.push(UploadResult::Text { filename: file.filename, text });
    }

    Ok(Json(UploadResponse { results }))
}

fn is_audio_video(ext: &str) -> bool {
    matches!(
        ext,
        "mp3" | "wav" | "m4a" | "ogg" | "flac" | "aac" | "wma" | "alac" | "aiff" | "opus"
            | "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "3gp" | "ts"
            | "mpeg" | "mpg" | "rm" | "rmvb" | "vob" | "mts" | "m2ts" | "divx"
    )
}

fn err(status: StatusCode, msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg }))
}
