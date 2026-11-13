use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};

use crate::config::Config;
use crate::deployment::Deployment;
use crate::domain::transcribe;
use crate::domain::upstream::{self, Catalog, Endpoint};
use crate::error::Error;
use crate::ratelimit::RateLimiter;
use crate::session::Sessions;
use crate::state::AppState;
use crate::{api, assets, security, web};

/// JSON bodies carry a text that escaping can grow sixfold.
const JSON_LIMIT: usize = 8 << 20;
/// A long text is translated chunk by chunk before the answer starts.
const SLOW: Duration = Duration::from_secs(1800);

pub async fn build(cfg: Config) -> Result<(Router, AppState), Error> {
    let ffmpeg = transcribe::ffmpeg_available().await;
    build_with(cfg, ffmpeg, Router::new())
}

/// Builds the router and state; `extra` holds routes a caller (or test) adds next to the app's own.
pub fn build_with(cfg: Config, ffmpeg: bool, extra: Router<AppState>) -> Result<(Router, AppState), Error> {
    cfg.validate()?;
    let dep = Deployment::from_config(&cfg)?;
    let st = AppState {
        cfg: Arc::new(cfg),
        dep: Arc::new(dep),
        http: upstream::http_client()?,
        catalog: Arc::default(),
        sessions: Arc::new(Sessions::default()),
        key_failures: Arc::new(RateLimiter::new(10, 5)),
        ffmpeg,
    };
    let api = api::routes()?;
    // Uploads get their own group so only they may be large and slow. The limits wrap the routes
    // themselves: a router merged in after would not get them.
    let uploads = api.uploads.with_state(st.clone());
    let uploads = security::limited(uploads, api::upload_limit(&st).saturating_add(1 << 20), Duration::from_secs(3600));
    let rest = Router::new()
        .merge(web::routes())
        .merge(api.rest)
        .merge(extra)
        .fallback(web::errors::not_found)
        .layer(from_fn_with_state(st.clone(), web::errors::render))
        .layer(from_fn_with_state(st.clone(), web::session))
        .with_state(st.clone());
    let rest = security::limited(rest, JSON_LIMIT, SLOW);
    let router = security::secure(
        uploads.merge(rest).merge(assets::router()).layer(from_fn(api::json_errors)),
        st.dep.clone(),
        st.cfg.trusted_proxies.clone(),
    );
    Ok((router, st))
}

/// Probes the server's own endpoints for their models in the background, so the first visitor
/// does not wait. Endpoints with configured model lists are not probed.
pub fn warm_catalog(st: &AppState) {
    let a = &st.cfg.app;
    let endpoints: Vec<Endpoint> = [&a.backend, &a.gated]
        .into_iter()
        .flatten()
        .filter(|b| b.models.is_empty() && b.transcription_models.is_empty())
        .map(|b| Endpoint::new(b.url.as_str(), b.api_key.as_deref()))
        .collect();
    for ep in endpoints {
        let (http, catalog): (_, Arc<Catalog>) = (st.http.clone(), st.catalog.clone());
        tokio::spawn(async move {
            match upstream::discover(&http, &ep, Some(&catalog)).await {
                Ok(models) => {
                    tracing::info!(endpoint = %ep.host(), translation = models.translation.len(), transcription = models.transcription.len(), "models found");
                    catalog.set_models(&ep, models);
                }
                Err(e) => tracing::warn!(endpoint = %ep.host(), detail = ?e.detail(), "the model list could not be fetched; only the configured default is offered"),
            }
        });
    }
}
