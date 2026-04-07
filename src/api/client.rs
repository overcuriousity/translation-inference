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
    /// Did not respond successfully to any probe.
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
    ///   1. Send text → expect text  (Translation)
    ///   2. Send audio → expect text (Transcription)
    ///   3. Send text  → expect audio (Tts)
    ///   4. None matched → Unknown
    ///
    /// This is intentionally behaviour-based so that unusual model names
    /// (e.g. a TTS model called "whisper-voice") are still classified correctly.
    pub async fn probe_model_kind(&self, model_id: &str) -> ModelKind {
        if self.probe_chat(model_id).await {
            return ModelKind::Translation;
        }
        if self.probe_stt(model_id).await {
            return ModelKind::Transcription;
        }
        if self.probe_tts(model_id).await {
            return ModelKind::Tts;
        }
        ModelKind::Unknown
    }

    /// Try `/v1/chat/completions` with a 1-token request.
    /// Returns true on a 2xx response.
    async fn probe_chat(&self, model_id: &str) -> bool {
        let body = serde_json::json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "x"}],
            "max_tokens": 1
        });
        matches!(
            self.http
                .post(self.chat_url())
                .bearer_auth(&self.api_key)
                .json(&body)
                .timeout(Duration::from_secs(10))
                .send()
                .await,
            Ok(r) if r.status().is_success()
        )
    }

    /// Try `/v1/audio/transcriptions` with a minimal silent WAV.
    /// Returns true if the response is 2xx JSON containing a "text" field.
    async fn probe_stt(&self, model_id: &str) -> bool {
        let wav = minimal_wav();
        let part = match reqwest::multipart::Part::bytes(wav)
            .file_name("probe.wav")
            .mime_str("audio/wav")
        {
            Ok(p) => p,
            Err(_) => return false,
        };
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
            Ok(r) if r.status().is_success() => r
                .json::<serde_json::Value>()
                .await
                .map(|v| v.get("text").is_some())
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Try `/v1/audio/speech` with a minimal text payload.
    /// Returns true if the response is 2xx with an audio Content-Type.
    async fn probe_tts(&self, model_id: &str) -> bool {
        let body = serde_json::json!({
            "model": model_id,
            "input": "x",
            "voice": "alloy"
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
            Ok(r) if r.status().is_success() => r
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|ct| ct.starts_with("audio/") || ct == "application/octet-stream")
                .unwrap_or(false),
            _ => false,
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
