use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
};

mod api;
mod config;
mod document;
mod models;
mod routes;
mod static_files;

use api::client::OpenAiClient;
use config::AppConfig;

/// Which backend tier a session was authenticated against.
#[derive(Clone, Copy)]
pub enum SessionTier {
    Byok,
    Gated,
}

/// Credentials stored for a browser session.
pub struct SessionCredentials {
    pub endpoint: String,
    pub api_key: String,
    pub tier: SessionTier,
}

pub struct AppState {
    pub config: AppConfig,
    pub client: OpenAiClient,
    /// Pre-built client for the gated tier (None if not configured).
    pub gated_client: Option<OpenAiClient>,
    /// Separate reqwest client for Bitvault proxy fetches with redirects disabled
    /// to prevent SSRF via open redirects on the Bitvault host.
    pub bitvault_http: reqwest::Client,
    /// In-memory session store keyed by session ID (set via `sid` cookie).
    /// Sessions survive as long as the server process runs; the browser-side
    /// cookie is session-scoped (no Max-Age) so it expires when the tab closes.
    pub sessions: std::sync::RwLock<std::collections::HashMap<String, SessionCredentials>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "translation_inference=info,tower_http=info".into()),
        )
        .init();

    check_ffmpeg()?;
    check_pdftotext();

    let config = AppConfig::from_env()?;
    let listen_addr = config.listen_addr.clone();

    if config.is_configured() {
        tracing::info!("Server configured with API endpoint: {}", config.api_base_url);
    } else {
        tracing::info!("No API credentials in environment — users must supply endpoint/key via the web UI");
    }

    let client = OpenAiClient::new(&config);
    let gated_client = if config.is_gated_configured() {
        tracing::info!("Gated tier configured with endpoint: {}", config.gated_api_base_url);
        Some(OpenAiClient::with_credentials(&config.gated_api_base_url, &config.gated_api_key))
    } else {
        None
    };
    let bitvault_http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to build Bitvault HTTP client");
    let state = Arc::new(AppState {
        config,
        client,
        gated_client,
        bitvault_http,
        sessions: std::sync::RwLock::new(std::collections::HashMap::new()),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Static files
        .route("/", get(static_files::serve_static))
        .route("/static/*path", get(static_files::serve_static))
        // API
        .route("/api/status", get(routes::config::get_status))
        .route("/api/config/test", post(routes::config::post_config_test))
        .route("/api/config/gated", post(routes::config::post_gated_access))
        .route("/api/translate", post(routes::translate::post_translate))
        .route("/api/translate/stream", post(routes::translate::post_translate_stream))
        .route("/api/transcribe", post(routes::transcribe::post_transcribe))
        .route("/api/translate-document", post(routes::document::post_translate_document))
        .route("/api/upload", post(routes::upload::post_upload))
        .route("/api/save-to-bitvault", post(routes::bitvault::post_save_to_bitvault))
        .route("/api/proxy-text", get(routes::bitvault::get_proxy_text))
        .route("/api/languages", get(routes::languages::get_languages))
        .route("/api/models", get(routes::models::get_models))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("Listening on http://{listen_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}

fn check_ffmpeg() -> Result<()> {
    use std::process::Command;
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(out) if out.status.success() => {
            tracing::info!("ffmpeg found");
            Ok(())
        }
        Ok(_) => anyhow::bail!("ffmpeg found but returned an error on -version"),
        Err(_) => anyhow::bail!(
            "ffmpeg not found in PATH. Please install ffmpeg (e.g. `apt install ffmpeg` or `dnf install ffmpeg`)"
        ),
    }
}

fn check_pdftotext() {
    if document::pdf::is_available() {
        tracing::info!("pdftotext found — PDF translation enabled");
    } else {
        tracing::warn!("pdftotext not found — PDF translation disabled (install poppler-utils to enable)");
    }
}
