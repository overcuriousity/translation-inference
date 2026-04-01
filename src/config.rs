use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub api_base_url: String,
    pub api_key: String,
    pub translation_model: String,
    pub whisper_model: String,
    pub translation_models: Vec<String>,
    pub whisper_models: Vec<String>,
    pub listen_addr: String,
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
            translation_model: std::env::var("TRANSLATION_MODEL")
                .unwrap_or_else(|_| "gpgpu/qwen3:14b-q5_k_m-32768".to_string()),
            whisper_model: std::env::var("WHISPER_MODEL")
                .unwrap_or_else(|_| "gpgpu/whisper".to_string()),
            translation_models,
            whisper_models,
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
        })
    }

    pub fn is_configured(&self) -> bool {
        !self.api_base_url.is_empty() && !self.api_key.is_empty()
    }
}
