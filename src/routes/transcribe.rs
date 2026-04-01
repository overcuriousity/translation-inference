use axum::{extract::{Multipart, State}, http::StatusCode, Json};
use std::sync::Arc;

use crate::api::whisper::{self, extract_audio_from_video, is_video_file};
use crate::models::{ErrorResponse, TranscribeResponse};
use crate::routes::translate::resolve_client;
use crate::AppState;

/// Maximum upload size: 100 MB
const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;

pub async fn post_transcribe(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<TranscribeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename = String::from("upload.bin");
    let mut model: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut api_key: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                if let Some(name) = field.file_name() {
                    filename = name.to_string();
                }
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if data.len() > MAX_UPLOAD_BYTES {
                    return Err(err(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("File exceeds {MAX_UPLOAD_BYTES} byte limit"),
                    ));
                }
                file_bytes = Some(data.to_vec());
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

    let bytes = file_bytes.ok_or_else(|| err(StatusCode::BAD_REQUEST, "No file field found".into()))?;
    let client = resolve_client(&state, endpoint.as_deref(), api_key.as_deref())?;

    // For video files, extract audio with ffmpeg first
    let (final_bytes, final_filename) = if is_video_file(&filename) {
        let wav = extract_audio_to_wav(bytes).await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        (wav, "extracted.wav".to_string())
    } else {
        (bytes, filename)
    };

    let model_id = model.unwrap_or_else(|| state.config.whisper_model.clone());

    match whisper::transcribe(&client, &model_id, final_bytes, &final_filename).await {
        Ok(text) => Ok(Json(TranscribeResponse { text })),
        Err(e) => {
            tracing::error!("Transcription error: {e:#}");
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

async fn extract_audio_to_wav(video_bytes: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    use std::io::Write;

    let mut input_tmp = tempfile::Builder::new()
        .suffix(".video")
        .tempfile()?;
    input_tmp.write_all(&video_bytes)?;

    let wav_tmp = extract_audio_from_video(input_tmp.path())?;
    let wav_bytes = std::fs::read(wav_tmp.path())?;
    Ok(wav_bytes)
}

fn err(status: StatusCode, msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg }))
}
