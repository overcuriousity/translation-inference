use axum::{extract::{Multipart, State}, http::{HeaderMap, StatusCode}, Json};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use std::path::Path;
use std::sync::Arc;

use crate::document::{self, OutputFormat};
use crate::models::{DocumentFile, ErrorResponse, TranslateDocumentResponse};
use crate::routes::translate::resolve_client;
use crate::AppState;

const MAX_DOC_BYTES: usize = 50 * 1024 * 1024; // 50 MB per file

pub async fn post_translate_document(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<TranslateDocumentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut source_lang = String::from("auto");
    let mut target_lang = String::from("English");
    let mut model: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut api_key: Option<String> = None;
    let mut output_format: Option<String> = None;

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
                if data.len() > MAX_DOC_BYTES {
                    return Err(err(StatusCode::PAYLOAD_TOO_LARGE, format!("{filename}: file too large")));
                }
                files.push((filename, data.to_vec()));
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
            Some("endpoint") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { endpoint = Some(v); }
            }
            Some("api_key") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { api_key = Some(v); }
            }
            Some("output_format") => {
                let v = field.text().await.map_err(|e| err(StatusCode::BAD_REQUEST, e.to_string()))?;
                if !v.is_empty() { output_format = Some(v); }
            }
            _ => {}
        }
    }

    if files.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "No files provided".into()));
    }

    let client = resolve_client(&state, endpoint.as_deref(), api_key.as_deref(), &headers)?;
    let model_str = model.as_deref().unwrap_or(&state.config.translation_model);

    let mut result_files: Vec<DocumentFile> = Vec::new();

    for (filename, bytes) in &files {
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");

        if !matches!(ext.as_str(), "pdf" | "odt" | "docx") {
            return Err(err(StatusCode::BAD_REQUEST, format!("Unsupported file type: .{ext}")));
        }

        let output_fmt = output_format
            .as_deref()
            .and_then(OutputFormat::from_str)
            .unwrap_or_else(|| OutputFormat::default_for(&ext));

        let (out, out_ext, mime) = document::translate_document(
            bytes,
            &ext,
            output_fmt,
            &client,
            model_str,
            &source_lang,
            &target_lang,
        )
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("{filename}: {e:#}")))?;

        result_files.push(DocumentFile {
            filename: format!("{stem}_translated.{out_ext}"),
            data: B64.encode(&out),
            mime: mime.into(),
        });
    }

    Ok(Json(TranslateDocumentResponse { files: result_files }))
}

fn err(status: StatusCode, msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg }))
}
