use crate::config::AppConfig;
use reqwest::Client;
use std::time::Duration;

/// The functional kind of a model, determined by probing its API endpoints.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModelKind {
    /// Accepts text, returns text — usable for translation.
    Translation,
    /// Accepts audio, returns text — speech-to-text.
    Transcription,
    /// Accepts text, returns audio — text-to-speech.
    Tts,
    /// All probes gave definitive non-success (4xx). Excluded from all dropdowns.
    Unknown,
}

#[derive(Clone)]
pub struct OpenAiClient {
    pub http: Client,
    pub base_url: String,
    pub api_key: String,
}

impl OpenAiClient {
    pub fn new(config: &AppConfig) -> Self {
        Self::with_credentials(&config.api_base_url, &config.api_key)
    }

    pub fn with_credentials(endpoint: &str, api_key: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(1800))
            .build()
            .expect("failed to build HTTP client");

        Self {
            http,
            base_url: endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        }
    }

    pub fn chat_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }

    pub fn transcription_url(&self) -> String {
        format!("{}/v1/audio/transcriptions", self.base_url)
    }

    pub fn speech_url(&self) -> String {
        format!("{}/v1/audio/speech", self.base_url)
    }

    pub fn models_url(&self) -> String {
        format!("{}/v1/models", self.base_url)
    }

    /// Probe a model sequentially to determine its kind:
    ///   1. POST /v1/chat/completions  (text in → text out)  → Translation
    ///   2. POST /v1/audio/transcriptions (audio in → text out) → Transcription
    ///   3. POST /v1/audio/speech      (text in → audio out) → Tts
    ///   4. All probes definitively failed                    → Unknown
    ///
    /// Returns `None` if any probe encountered a transient condition (429, 5xx,
    /// or network/timeout error) — the caller should **not** cache this result
    /// and should retry on the next opportunity.
    ///
    /// `tts_voice` should be one of the voices configured for this endpoint in
    /// the TTS_VOICE_MAP; falls back to "alloy" if not provided.
    pub async fn probe_model_kind(
        &self,
        model_id: &str,
        tts_voice: Option<&str>,
    ) -> Option<ModelKind> {
        match self.probe_chat(model_id).await? {
            true => return Some(ModelKind::Translation),
            false => {}
        }
        match self.probe_stt(model_id).await? {
            true => return Some(ModelKind::Transcription),
            false => {}
        }
        match self
            .probe_tts(model_id, tts_voice.unwrap_or("alloy"))
            .await?
        {
            true => return Some(ModelKind::Tts),
            false => {}
        }
        // All probes returned definitive non-success — model supports none of these APIs.
        Some(ModelKind::Unknown)
    }

    /// Try `/v1/chat/completions` with a 1-token request.
    ///
    /// Returns `Some(true)` on 2xx, `Some(false)` on a definitive 4xx (not 429),
    /// and `None` on 429 / 5xx / network error (transient — do not cache).
    async fn probe_chat(&self, model_id: &str) -> Option<bool> {
        let body = serde_json::json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "x"}],
            "max_tokens": 1
        });
        match self
            .http
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(r) => {
                let status = r.status();
                let _ = r.bytes().await; // drain body to allow connection reuse
                if status.is_success() {
                    Some(true)
                } else if status.as_u16() == 429 || status.is_server_error() {
                    None // transient
                } else {
                    Some(false) // 404, 400, 422, … — definitively not a chat model
                }
            }
            Err(_) => None, // timeout / network error — transient
        }
    }

    /// Try `/v1/audio/transcriptions` with a minimal silent WAV.
    ///
    /// Returns `Some(true)` if the response is 2xx JSON with a "text" field,
    /// `Some(false)` on definitive 4xx, `None` on transient conditions.
    async fn probe_stt(&self, model_id: &str) -> Option<bool> {
        let wav = minimal_wav();
        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("probe.wav")
            .mime_str("audio/wav")
            .ok()?;
        let form = reqwest::multipart::Form::new()
            .text("model", model_id.to_string())
            .part("file", part);
        match self
            .http
            .post(self.transcription_url())
            .bearer_auth(&self.api_key)
            .multipart(form)
            .timeout(Duration::from_secs(15))
            .send()
            .await
        {
            Ok(r) => {
                let status = r.status();
                if status.as_u16() == 429 || status.is_server_error() {
                    return None; // transient
                }
                if !status.is_success() {
                    return Some(false); // definitive
                }
                let has_text = r
                    .json::<serde_json::Value>()
                    .await
                    .map(|v| v.get("text").is_some())
                    .unwrap_or(false);
                Some(has_text)
            }
            Err(_) => None, // transient
        }
    }

    /// Try `/v1/audio/speech` with a minimal text payload.
    ///
    /// Returns `Some(true)` if the response is 2xx with an audio Content-Type,
    /// `Some(false)` on definitive 4xx, `None` on transient conditions.
    async fn probe_tts(&self, model_id: &str, voice: &str) -> Option<bool> {
        let body = serde_json::json!({
            "model": model_id,
            "input": "x",
            "voice": voice
        });
        match self
            .http
            .post(self.speech_url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .timeout(Duration::from_secs(15))
            .send()
            .await
        {
            Ok(r) => {
                let status = r.status();
                if status.as_u16() == 429 || status.is_server_error() {
                    return None; // transient
                }
                let is_audio = status.is_success()
                    && r.headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map(|ct| ct.starts_with("audio/") || ct == "application/octet-stream")
                        .unwrap_or(false);
                let _ = r.bytes().await; // drain body to allow connection reuse
                Some(is_audio)
            }
            Err(_) => None, // transient
        }
    }

    pub async fn fetch_models(&self) -> anyhow::Result<Vec<String>> {
        let res = self
            .http
            .get(self.models_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !res.status().is_success() {
            anyhow::bail!("Failed to fetch models: {}", res.status());
        }

        let body: crate::models::OpenAiModelsResponse = res.json().await?;
        Ok(body.data.into_iter().map(|m| m.id).collect())
    }
}

/// Build a minimal valid WAV: 8 kHz, 16-bit, mono, 0.1 s of silence.
/// Used by the STT probe to avoid sending real audio data.
fn minimal_wav() -> Vec<u8> {
    let sample_rate: u32 = 8000;
    let num_samples: u32 = 800; // 0.1 second
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample
    let chunk_size = 36 + data_size;
    let mut wav = Vec::with_capacity((44 + data_size) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&chunk_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // subchunk1size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.resize(wav.len() + data_size as usize, 0u8); // silence
    wav
}
