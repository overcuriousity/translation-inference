use serde::{Deserialize, Serialize};

// ── Inbound API requests ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigTestRequest {
    pub endpoint: String,
    pub api_key: String,
}

// ── Outbound API responses ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TranslateResponse {
    pub translated_text: String,
    pub chunks_total: usize,
    pub chunks_completed: usize,
}

#[derive(Debug, Serialize)]
pub struct TranscribeResponse {
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct LanguagesResponse {
    pub languages: Vec<Language>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Language {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub translation_models: Vec<ModelInfo>,
    pub transcription_models: Vec<ModelInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiModelsResponse {
    pub data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiModel {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub server_configured: bool,
    /// True when the request carries a valid `sid` session cookie.
    pub session_active: bool,
    pub bitvault_configured: bool,
}

#[derive(Debug, Deserialize)]
pub struct SaveToBitvaultRequest {
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct SaveToBitvaultResponse {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigTestResponse {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DocumentFile {
    pub filename: String,
    pub data: String, // base64
    pub mime: String,
}

#[derive(Debug, Serialize)]
pub struct TranslateDocumentResponse {
    pub files: Vec<DocumentFile>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UploadResult {
    Text { filename: String, text: String },
    Document { filename: String, data: String, mime: String },
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub results: Vec<UploadResult>,
}

// ── OpenAI-compatible structures ────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
}

#[derive(Debug, Deserialize)]
pub struct StreamResponse {
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    pub delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
pub struct StreamDelta {
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WhisperResponse {
    pub text: String,
}
