use crate::config::AppConfig;
use reqwest::Client;
use std::time::Duration;

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

    pub fn models_url(&self) -> String {
        format!("{}/v1/models", self.base_url)
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
