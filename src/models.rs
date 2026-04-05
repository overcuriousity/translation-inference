use serde::{Deserialize, Serialize};

// ── Inbound API requests ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub context: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TranslateParagraphsRequest {
    pub text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub context: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParagraphPair {
    pub source: String,
    pub translation: String,
}

#[derive(Debug, Serialize)]
pub struct TranslateParagraphsResponse {
    pub paragraphs: Vec<ParagraphPair>,
    pub chunks_total: usize,
    pub chunks_completed: usize,
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
    pub gated_configured: bool,
    /// True when the request carries a valid `sid` session cookie.
    pub session_active: bool,
    /// Which tier the active session belongs to: "byok" or "gated".
    pub session_tier: Option<String>,
    pub bitvault_configured: bool,
    pub tts_configured: bool,
    /// Language codes that have a TTS voice configured (keys of TTS_VOICE_MAP).
    pub tts_languages: Vec<String>,
    /// Hostname of the TTS endpoint for UI display (e.g. "tts.example.com"). None if not configured.
    pub tts_hostname: Option<String>,
    /// Representative TTS model string for UI display (first alphabetical from TTS_VOICE_MAP). None if not configured.
    pub tts_model: Option<String>,
    /// Input character limit applicable to this session. None = unlimited.
    pub char_limit: Option<usize>,
    /// Short git commit hash baked in at compile time.
    pub git_commit: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct TtsRequest {
    pub text: String,
    /// Required language code used to select a voice+model from TTS_VOICE_MAP (e.g. "en", "de").
    pub target_lang: String,
    /// Optional BYOK override — TTS endpoint base URL.
    pub tts_endpoint: Option<String>,
    /// Optional BYOK override — TTS API key.
    pub tts_api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DetectLanguageRequest {
    pub text: String,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetectLanguageResponse {
    pub language: String,
}

#[derive(Debug, Deserialize)]
pub struct GatedAccessRequest {
    pub access_key: String,
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
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UploadResult {
    Text { filename: String, text: String },
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
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StreamResponse {
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    pub delta: StreamDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StreamDelta {
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WhisperResponse {
    pub text: String,
}
