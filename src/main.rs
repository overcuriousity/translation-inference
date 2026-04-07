use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
};

mod api;
mod config;
mod models;
mod routes;
mod static_files;
mod subtitle;

use api::client::OpenAiClient;
use config::AppConfig;

/// Which backend tier a session was authenticated against.
#[derive(Clone, Copy)]
pub enum SessionTier {
    /// Anonymous web-UI session: uses the server's own backend credentials.
    /// Grants access to text translation and conversation only; no file tab.
    Free,
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
    /// Pre-built client for the TTS endpoint (None if not configured).
    pub tts_client: Option<OpenAiClient>,
    /// Separate reqwest client for Bitvault proxy fetches with redirects disabled
    /// to prevent SSRF via open redirects on the Bitvault host.
    pub bitvault_http: reqwest::Client,
    /// In-memory session store keyed by session ID (set via `sid` cookie).
    /// Sessions survive as long as the server process runs; the browser-side
    /// cookie is session-scoped (no Max-Age) so it expires when the tab closes.
    pub sessions: std::sync::RwLock<std::collections::HashMap<String, SessionCredentials>>,
    /// Cache: (endpoint_base_url, model_id) → ModelKind.
    /// Populated at startup for the server's own models; BYOK models are probed
    /// on first `/api/models` call and cached here for subsequent requests.
    pub model_capabilities:
        std::sync::RwLock<std::collections::HashMap<(String, String), api::client::ModelKind>>,
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

    let config = AppConfig::from_env()?;
    let listen_addr = config.listen_addr.clone();

    if config.is_configured() {
        tracing::info!(
            "Server configured with API endpoint: {}",
            config.api_base_url
        );
    } else {
        tracing::info!(
            "No API credentials in environment — users must supply endpoint/key via the web UI"
        );
    }

    let client = OpenAiClient::new(&config);
    let gated_client = if config.is_gated_configured() {
        tracing::info!(
            "Gated tier configured with endpoint: {}",
            config.gated_api_base_url
        );
        Some(OpenAiClient::with_credentials(
            &config.gated_api_base_url,
            &config.gated_api_key,
        ))
    } else {
        None
    };
    let tts_client = if config.is_tts_configured() {
        tracing::info!("TTS configured with endpoint: {}", config.tts_api_base_url);
        Some(OpenAiClient::with_credentials(
            &config.tts_api_base_url,
            &config.tts_api_key,
        ))
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
        tts_client,
        bitvault_http,
        sessions: std::sync::RwLock::new(std::collections::HashMap::new()),
        model_capabilities: std::sync::RwLock::new(std::collections::HashMap::new()),
    });

    // Probe the server's own models at startup so the capability cache is warm
    // before the first user request. Each model is probed sequentially:
    // chat → STT → TTS → Unknown. Only definitive results are cached;
    // transient failures (429 / 5xx / timeout) are skipped so the model is
    // re-probed on the next /api/models call.
    {
        let probe_state = state.clone();
        tokio::spawn(async move {
            // Use a configured TTS voice so the TTS probe is as realistic as possible.
            let tts_voice: Option<String> = probe_state
                .config
                .tts_voice_map
                .values()
                .next()
                .map(|e| e.voice.clone());
            let tts_voice_ref = tts_voice.as_deref();

            let client = probe_state.client.clone();
            match client.fetch_models().await {
                Ok(models) => {
                    for id in models {
                        match client.probe_model_kind(&id, tts_voice_ref).await {
                            Some(kind) => {
                                tracing::info!(model = %id, kind = ?kind, "model capability probe");
                                probe_state
                                    .model_capabilities
                                    .write()
                                    .unwrap()
                                    .insert((client.base_url.clone(), id), kind);
                            }
                            None => {
                                tracing::warn!(model = %id, "startup probe inconclusive (transient); will retry on first request");
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("startup model probe failed: {e:#}"),
            }
        });
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Static files
        .route("/", get(static_files::serve_index))
        .route("/static/*path", get(static_files::serve_static))
        // API docs
        .route("/openapi.yaml", get(static_files::get_openapi_spec))
        .route("/docs", get(static_files::get_swagger_docs))
        // API
        .route("/api/status", get(routes::config::get_status))
        .route("/api/config/test", post(routes::config::post_config_test))
        .route("/api/config/check", post(routes::config::post_config_check))
        .route("/api/config/gated", post(routes::config::post_gated_access))
        .route("/api/translate", post(routes::translate::post_translate))
        .route(
            "/api/translate/stream",
            post(routes::translate::post_translate_stream),
        )
        .route(
            "/api/translate/paragraphs",
            post(routes::translate_paragraphs::post_translate_paragraphs),
        )
        .route(
            "/api/translate-subtitle",
            post(routes::subtitle::post_translate_subtitle),
        )
        .route("/api/transcribe", post(routes::transcribe::post_transcribe))
        .route("/api/upload", post(routes::upload::post_upload))
        .route(
            "/api/save-to-bitvault",
            post(routes::bitvault::post_save_to_bitvault),
        )
        .route("/api/proxy-text", get(routes::bitvault::get_proxy_text))
        .route("/api/languages", get(routes::languages::get_languages))
        .route("/api/models", get(routes::models::get_models))
        .route("/api/tts", post(routes::tts::post_tts))
        .route(
            "/api/detect-language",
            post(routes::detect_language::post_detect_language),
        )
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                Json(models::ErrorResponse {
                    error: "Not found".into(),
                }),
            )
        })
        .method_not_allowed_fallback(|| async {
            (
                StatusCode::METHOD_NOT_ALLOWED,
                Json(models::ErrorResponse {
                    error: "Method not allowed".into(),
                }),
            )
        })
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
