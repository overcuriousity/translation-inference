use anyhow::Result;

/// A single entry in `TTS_VOICE_MAP`. The `voice` is the voice ID passed to the
/// TTS endpoint. `model` optionally overrides the TTS model for this language,
/// allowing different backends (e.g. a Piper model for German, Kokoro for English)
/// to coexist behind the same speaches-ai instance.
#[derive(Debug, Clone)]
pub struct TtsVoiceEntry {
    pub voice: String,
    /// When `Some`, this model ID is sent in the TTS request instead of the
    /// server-wide fallback. Use `voice@model` syntax in `TTS_VOICE_MAP`.
    pub model: Option<String>,
}

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
    /// Fallback TTS model used when the matched `TTS_VOICE_MAP` entry has no
    /// model override, or when no map entry exists for the requested language.
    /// Prefer using `voice@model` entries in `TTS_VOICE_MAP` instead of setting
    /// this directly (`TTS_MODEL` env var is deprecated).
    pub tts_model: String,
    /// Fallback voice used when no `TTS_VOICE_MAP` entry matches the language.
    /// Prefer adding an explicit entry to `TTS_VOICE_MAP` instead
    /// (`TTS_VOICE` env var is deprecated).
    pub tts_voice: String,
    /// Maximum **bytes** per TTS request chunk. `None` (the default) disables
    /// chunking entirely — recommended for local models that have no per-request
    /// size limit. Set `TTS_CHUNK_SIZE=4000` to restore OpenAI hosted-API limits;
    /// `TTS_CHUNK_SIZE=0` also means no chunking.
    pub tts_chunk_size: Option<usize>,
    /// Per-language TTS voice (and optionally model) overrides.
    /// Populated from `TTS_VOICE_MAP=en:af_heart@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,...`
    /// Format per entry: `lang:voice` or `lang:voice@model`.
    /// The map keys determine which languages show TTS buttons in the UI.
    pub tts_voice_map: std::collections::HashMap<String, TtsVoiceEntry>,
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
            tts_model: std::env::var("TTS_MODEL").unwrap_or_else(|_| {
                // TTS_MODEL is deprecated — prefer voice@model entries in TTS_VOICE_MAP.
                "speaches-ai/Kokoro-82M-v1.0-ONNX-fp16".to_string()
            }),
            tts_voice: std::env::var("TTS_VOICE").unwrap_or_else(|_| {
                // TTS_VOICE is deprecated — prefer explicit entries in TTS_VOICE_MAP.
                "af_heart".to_string()
            }),
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
                    let value = it.next()?.trim().to_string();
                    if raw_lang.is_empty() || value.is_empty() { return None; }
                    // Parse `voice@model` or plain `voice`.
                    let (voice, model) = match value.find('@') {
                        Some(at) => {
                            let v = value[..at].trim().to_string();
                            let m = value[at + 1..].trim().to_string();
                            (v, if m.is_empty() { None } else { Some(m) })
                        }
                        None => (value, None),
                    };
                    if voice.is_empty() { return None; }
                    // Normalise to canonical casing (e.g. zh-tw → zh-TW) so keys
                    // match what /api/languages and the frontend use.
                    let lang = match raw_lang.to_lowercase().as_str() {
                        "zh-tw" => "zh-TW".to_string(),
                        other   => other.to_string(),
                    };
                    Some((lang, TtsVoiceEntry { voice, model }))
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
