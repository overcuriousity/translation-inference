use axum::{extract::{Multipart, State}, http::{HeaderMap, StatusCode}, Json};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use crate::api::whisper::{self, extract_audio_from_video, is_video_file};
use crate::models::{ErrorResponse, TranscribeResponse};
use crate::routes::translate::resolve_client;
use crate::AppState;

/// Maximum upload size: 100 MB
const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;

pub async fn post_transcribe(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<TranscribeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut file_tmp: Option<NamedTempFile> = None;
    let mut filename = String::from("upload.bin");
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
                if let Some(name) = field.file_name() {
                    filename = name.to_string();
                }
                
                let ext = std::path::Path::new(&filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("bin");
                
                let tmp = tempfile::Builder::new()
                    .suffix(&format!(".{ext}"))
                    .tempfile()
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                
                let mut tmp_file = tokio::fs::File::from(
                    tmp.reopen().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                );
                let mut size = 0;
                
                while let Some(chunk) = field.chunk().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))? {
                    size += chunk.len();
                    if size > MAX_UPLOAD_BYTES {
                        return Err(err(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            format!("File exceeds {MAX_UPLOAD_BYTES} byte limit"),
                        ));
                    }
                    tmp_file.write_all(&chunk).await.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                }
                tmp_file.flush().await.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                
                file_tmp = Some(tmp);
            }
            Some("model") => {
                model = Some(
                    field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?,
                );
            }
            Some("endpoint") => {
                endpoint = Some(
                    field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?,
                );
            }
            Some("api_key") => {
                api_key = Some(
                    field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?,
                );
            }
            _ => {}
        }
    }

    let file_tmp = file_tmp.ok_or_else(|| err(StatusCode::BAD_REQUEST, "No file field found".into()))?;
    let client = resolve_client(&state, endpoint.as_deref(), api_key.as_deref(), &headers)?;

    // For video files, extract audio with ffmpeg first
    let mut final_filename = filename.clone();
    let final_tmp = if is_video_file(&filename) {
        let wav_tmp = extract_audio_from_video(file_tmp.path())
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        final_filename = "extracted.wav".to_string();
        wav_tmp
    } else {
        file_tmp
    };

    let model_id = model.unwrap_or_else(|| state.config.whisper_model.clone());

    match whisper::transcribe(&client, &model_id, final_tmp.path(), &final_filename).await {
        Ok(text) => Ok(Json(TranscribeResponse { text })),
        Err(e) => {
            tracing::error!("Transcription error: {e:#}");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

fn err(status: StatusCode, msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg }))
}
