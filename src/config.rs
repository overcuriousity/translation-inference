use anyhow::Result;

/// A single entry in `TTS_VOICE_MAP`.
/// Parsed from `lang:voice@model` in the `TTS_VOICE_MAP` environment variable.
#[derive(Debug, Clone)]
pub struct TtsVoiceEntry {
    pub voice: String,
    pub model: String,
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
    /// Maximum **bytes** per TTS request chunk. `None` (the default) disables
    /// chunking entirely — recommended for local models that have no per-request
    /// size limit. Set `TTS_CHUNK_SIZE=4000` to restore OpenAI hosted-API limits;
    /// `TTS_CHUNK_SIZE=0` also means no chunking.
    pub tts_chunk_size: Option<usize>,
    /// Per-language TTS voice and model configuration.
    /// Populated from `TTS_VOICE_MAP=en:af_heart@speaches-ai/Kokoro-82M-v1.0-ONNX-fp16,...`
    /// Required format per entry: `lang:voice@model`.
    /// The map keys determine which languages show TTS buttons in the UI.
    pub tts_voice_map: std::collections::HashMap<String, TtsVoiceEntry>,

    // ── Translation quality tuning ────────────────────────────────────────────
    /// Fallback context size (tokens) when the model ID does not encode one.
    /// Default: 4096. Override with `DEFAULT_CONTEXT_SIZE`.
    pub default_context_size: usize,
    /// Fraction of usable tokens allocated to input (remainder goes to output).
    /// Default: 0.5. Set to e.g. 0.4 for compact→verbose language pairs.
    /// Override with `INPUT_TOKEN_RATIO`.
    pub input_token_ratio: f64,
    /// Chars-per-token ratio for CJK-dominant text. Default: 1.5.
    /// Override with `CJK_CHARS_PER_TOKEN`.
    pub cjk_chars_per_token: f64,
    /// Chars-per-token ratio for Latin-script text. Default: 4.0.
    /// Override with `LATIN_CHARS_PER_TOKEN`.
    pub latin_chars_per_token: f64,
    /// Warn when translated output/input char ratio is below this value.
    /// Default: 0.3. Override with `MIN_OUTPUT_RATIO`.
    pub min_output_ratio: f64,

    // ── Per-tier input character limits ──────────────────────────────────────
    /// Maximum input characters for the free tier (anonymous web-UI sessions).
    /// `None` = unlimited. Default: 16 000 (~4 096 tokens). Set `FREE_TIER_CHAR_LIMIT=0` to disable.
    pub free_tier_char_limit: Option<usize>,
    /// Maximum input characters for gated-key sessions.
    /// `None` = unlimited. Default: 65 536 (~16 384 tokens). Set `GATED_CHAR_LIMIT=0` to disable.
    pub gated_char_limit: Option<usize>,
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
            tts_chunk_size: match std::env::var("TTS_CHUNK_SIZE").ok().as_deref() {
                Some("0") | None => None,
                Some(s) => s.parse::<usize>().ok().filter(|&n| n > 0),
            },
            default_context_size: std::env::var("DEFAULT_CONTEXT_SIZE")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .map(|n| n.max(1024))
                .unwrap_or(4096),
            input_token_ratio: std::env::var("INPUT_TOKEN_RATIO")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v.clamp(0.1, 0.9))
                .unwrap_or(0.5),
            cjk_chars_per_token: std::env::var("CJK_CHARS_PER_TOKEN")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v.max(0.1))
                .unwrap_or(1.5),
            latin_chars_per_token: std::env::var("LATIN_CHARS_PER_TOKEN")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v.max(0.1))
                .unwrap_or(4.0),
            min_output_ratio: std::env::var("MIN_OUTPUT_RATIO")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v.max(0.0))
                .unwrap_or(0.3),
            free_tier_char_limit: match std::env::var("FREE_TIER_CHAR_LIMIT") {
                Ok(s) => {
                    let s = s.trim();
                    if s == "0" {
                        None
                    } else {
                        match s.parse::<usize>() {
                            Ok(n) if n > 0 => Some(n),
                            _ => {
                                eprintln!("Invalid FREE_TIER_CHAR_LIMIT value {s:?}; using default 16000");
                                Some(16_000)
                            }
                        }
                    }
                }
                Err(_) => Some(16_000),
            },
            gated_char_limit: match std::env::var("GATED_CHAR_LIMIT") {
                Ok(s) => {
                    let s = s.trim();
                    if s == "0" {
                        None
                    } else {
                        match s.parse::<usize>() {
                            Ok(n) if n > 0 => Some(n),
                            _ => {
                                eprintln!("Invalid GATED_CHAR_LIMIT value {s:?}; using default 65536");
                                Some(65_536)
                            }
                        }
                    }
                }
                Err(_) => Some(65_536),
            },
            tts_voice_map: std::env::var("TTS_VOICE_MAP")
                .unwrap_or_default()
                .split(',')
                .filter_map(|pair| {
                    let mut it = pair.trim().splitn(2, ':');
                    let raw_lang = it.next()?.trim().to_string();
                    let value = it.next()?.trim().to_string();
                    if raw_lang.is_empty() || value.is_empty() { return None; }
                    // Require `voice@model` format; silently skip malformed entries.
                    let at = value.find('@')?;
                    let voice = value[..at].trim().to_string();
                    let model = value[at + 1..].trim().to_string();
                    if voice.is_empty() || model.is_empty() { return None; }
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

    /// Returns the hostname of the TTS endpoint for UI display, or `None` if not configured.
    pub fn tts_hostname(&self) -> Option<String> {
        if !self.is_tts_configured() { return None; }
        url::Url::parse(&self.tts_api_base_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .filter(|h| !h.is_empty())
    }
}
