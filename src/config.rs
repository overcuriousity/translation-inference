use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub api_base_url: String,
    pub api_key: String,
    pub gated_api_base_url: String,
    pub gated_api_key: String,
    pub gated_access_key: String,
    pub translation_model: String,
    pub whisper_model: String,
    pub translation_models: Vec<String>,
    pub whisper_models: Vec<String>,
    pub listen_addr: String,
    pub bitvault_url: Option<String>,
    pub bitvault_api_key: Option<String>,
    pub tts_api_base_url: String,
    pub tts_api_key: String,
    pub tts_voice: String,
    /// Maximum **bytes** per TTS request chunk. `None` (the default) disables
    /// chunking entirely — recommended for local models (e.g. Qwen3-TTS) that
    /// have no per-request size limit. Set `TTS_CHUNK_SIZE=4000` to restore
    /// OpenAI-API-compatible behaviour; `TTS_CHUNK_SIZE=0` also means no chunking.
    pub tts_chunk_size: Option<usize>,
    /// Maps language codes to Piper voice names, e.g. `en` → `en_US-lessac-medium`.
    /// Populated from `TTS_VOICE_MAP=en:en_US-lessac-medium,de:de_DE-thorsten-medium,...`
    pub tts_voice_map: std::collections::HashMap<String, String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let translation_models = std::env::var("TRANSLATION_MODELS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let whisper_models = std::env::var("WHISPER_MODELS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        Ok(Self {
            api_base_url: std::env::var("API_BASE_URL").unwrap_or_default(),
            api_key: std::env::var("API_KEY").unwrap_or_default(),
            gated_api_base_url: std::env::var("GATED_API_BASE_URL").unwrap_or_default(),
            gated_api_key: std::env::var("GATED_API_KEY").unwrap_or_default(),
            gated_access_key: std::env::var("GATED_ACCESS_KEY").unwrap_or_default(),
            translation_model: std::env::var("TRANSLATION_MODEL")
                .unwrap_or_else(|_| "gpgpu/qwen3:14b-q5_k_m-32768".to_string()),
            whisper_model: std::env::var("WHISPER_MODEL")
                .unwrap_or_else(|_| "gpgpu/whisper".to_string()),
            translation_models,
            whisper_models,
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            bitvault_url: std::env::var("BITVAULT_URL")
                .ok()
                .map(|s| s.trim().trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty()),
            bitvault_api_key: std::env::var("BITVAULT_API_KEY")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            tts_api_base_url: std::env::var("TTS_API_BASE_URL").unwrap_or_default(),
            tts_api_key: std::env::var("TTS_API_KEY").unwrap_or_default(),
            tts_voice: std::env::var("TTS_VOICE")
                .unwrap_or_else(|_| "alloy".to_string()),
            tts_chunk_size: match std::env::var("TTS_CHUNK_SIZE").ok().as_deref() {
                Some("0") | None => None,
                Some(s) => s.parse::<usize>().ok().filter(|&n| n > 0),
            },
            tts_voice_map: std::env::var("TTS_VOICE_MAP")
                .unwrap_or_default()
                .split(',')
                .filter_map(|pair| {
                    let mut it = pair.trim().splitn(2, ':');
                    let raw_lang = it.next()?.trim().to_string();
                    let voice = it.next()?.trim().to_string();
                    if raw_lang.is_empty() || voice.is_empty() { return None; }
                    // Normalise to canonical casing (e.g. zh-tw → zh-TW) so keys
                    // match what /api/languages and the frontend use.
                    let lang = match raw_lang.to_lowercase().as_str() {
                        "zh-tw" => "zh-TW".to_string(),
                        other   => other.to_string(),
                    };
                    Some((lang, voice))
                })
                .collect(),
        })
    }

    pub fn is_configured(&self) -> bool {
        !self.api_base_url.is_empty() && !self.api_key.is_empty()
    }

    pub fn is_gated_configured(&self) -> bool {
        !self.gated_api_base_url.is_empty()
            && !self.gated_api_key.is_empty()
            && !self.gated_access_key.is_empty()
    }

    pub fn is_bitvault_configured(&self) -> bool {
        self.bitvault_url.is_some()
    }

    pub fn is_tts_configured(&self) -> bool {
        !self.tts_api_base_url.is_empty() && !self.tts_api_key.is_empty()
    }
}
