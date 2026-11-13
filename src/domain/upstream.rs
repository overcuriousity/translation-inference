//! The OpenAI-compatible endpoints this server talks to, and what their models can do.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use reqwest::{Client, RequestBuilder};
use serde::Deserialize;

use crate::error::Error;

/// One endpoint's base URL and key. Requests go to `{base}/v1/...`.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub base: String,
    pub key: Option<String>,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint").field("base", &self.base).finish_non_exhaustive()
    }
}

impl Endpoint {
    pub fn new(base: &str, key: Option<&str>) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            key: key.map(str::trim).filter(|k| !k.is_empty()).map(str::to_owned),
        }
    }

    pub fn host(&self) -> String {
        url::Url::parse(&self.base).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default()
    }

    pub fn post(&self, http: &Client, path: &str) -> RequestBuilder {
        self.auth(http.post(format!("{}{path}", self.base)))
    }

    pub fn get(&self, http: &Client, path: &str) -> RequestBuilder {
        self.auth(http.get(format!("{}{path}", self.base)))
    }

    fn auth(&self, rb: RequestBuilder) -> RequestBuilder {
        match &self.key {
            Some(k) => rb.bearer_auth(k),
            None => rb,
        }
    }
}

/// The one HTTP client for every upstream. Redirects are not followed: an endpoint the visitor
/// names must not bounce the server somewhere else.
pub fn http_client() -> Result<Client, Error> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(Error::internal)
}

/// Turns a failed request or a non-success answer into an error the visitor can act on.
pub async fn check(res: Result<reqwest::Response, reqwest::Error>, what: &str) -> Result<reqwest::Response, Error> {
    let res = res.map_err(|e| Error::upstream(format!("The {what} endpoint could not be reached. Try again later."), e))?;
    let status = res.status();
    if status.is_success() {
        return Ok(res);
    }
    let body: String = res.text().await.unwrap_or_default().chars().take(500).collect();
    let message = match status.as_u16() {
        401 | 403 => format!("The {what} endpoint refused the API key. Check it in Settings."),
        404 => format!("The {what} endpoint does not offer this. Check its address and model."),
        429 => format!("The {what} endpoint is busy. Wait a minute and try again."),
        _ => format!("The {what} endpoint answered with an error ({status}). Try again later."),
    };
    Err(Error::upstream(message, format!("{status}: {body}")))
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

pub async fn fetch_models(http: &Client, ep: &Endpoint) -> Result<Vec<String>, Error> {
    let res = check(ep.get(http, "/v1/models").timeout(Duration::from_secs(15)).send().await, "model").await?;
    let list: ModelList = res.json().await.map_err(|e| Error::upstream("The model list could not be read.", e))?;
    Ok(list.data.into_iter().map(|m| m.id).collect())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Translation,
    Transcription,
    Speech,
    /// Every probe said no.
    Unknown,
}

/// The models an endpoint offers for translation and for transcription.
#[derive(Clone, Debug, Default)]
pub struct Models {
    pub translation: Vec<String>,
    pub transcription: Vec<String>,
}

/// Probe results per (endpoint, model), shared by every visitor of the server's own endpoints.
#[derive(Default)]
pub struct Catalog {
    kinds: RwLock<HashMap<(String, String), Kind>>,
    lists: RwLock<HashMap<String, Models>>,
}

impl Catalog {
    pub fn models(&self, ep: &Endpoint) -> Option<Models> {
        self.lists.read().unwrap_or_else(|e| e.into_inner()).get(&ep.base).cloned()
    }

    pub fn set_models(&self, ep: &Endpoint, models: Models) {
        self.lists.write().unwrap_or_else(|e| e.into_inner()).insert(ep.base.clone(), models);
    }

    fn kind(&self, ep: &Endpoint, id: &str) -> Option<Kind> {
        self.kinds.read().unwrap_or_else(|e| e.into_inner()).get(&(ep.base.clone(), id.to_owned())).copied()
    }

    fn set_kind(&self, ep: &Endpoint, id: &str, kind: Kind) {
        self.kinds.write().unwrap_or_else(|e| e.into_inner()).insert((ep.base.clone(), id.to_owned()), kind);
    }
}

/// Lists an endpoint's models and sorts them by what they answer to. With a catalog, results are
/// cached; a visitor's own endpoint is probed without one, so nobody's answers reach another.
pub async fn discover(http: &Client, ep: &Endpoint, catalog: Option<&Catalog>) -> Result<Models, Error> {
    let mut out = Models::default();
    for id in fetch_models(http, ep).await? {
        let kind = match catalog.and_then(|c| c.kind(ep, &id)) {
            Some(k) => k,
            None => match probe(http, ep, &id).await {
                Some(k) => {
                    if let Some(c) = catalog {
                        c.set_kind(ep, &id, k);
                    }
                    tracing::info!(endpoint = %ep.host(), model = %id, kind = ?k, "model probed");
                    k
                }
                None => {
                    tracing::warn!(endpoint = %ep.host(), model = %id, "model probe inconclusive; it is left out for now");
                    continue;
                }
            },
        };
        match kind {
            Kind::Translation => out.translation.push(id),
            Kind::Transcription => out.transcription.push(id),
            Kind::Speech | Kind::Unknown => {}
        }
    }
    Ok(out)
}

/// Tries chat, then transcription, then speech. `None` when a probe met a transient failure
/// (429, 5xx, a timeout), so the result must not be cached.
pub async fn probe(http: &Client, ep: &Endpoint, model: &str) -> Option<Kind> {
    let mut inconclusive = false;
    for kind in [Kind::Translation, Kind::Transcription, Kind::Speech] {
        match probe_one(http, ep, model, kind).await {
            Some(true) => return Some(kind),
            Some(false) => {}
            None => inconclusive = true,
        }
    }
    (!inconclusive).then_some(Kind::Unknown)
}

async fn probe_one(http: &Client, ep: &Endpoint, model: &str, kind: Kind) -> Option<bool> {
    let req = match kind {
        Kind::Translation => ep.post(http, "/v1/chat/completions").json(&serde_json::json!({
            "model": model, "messages": [{"role": "user", "content": "x"}], "max_tokens": 1
        })),
        Kind::Transcription => {
            let part = reqwest::multipart::Part::bytes(silent_wav()).file_name("probe.wav").mime_str("audio/wav").ok()?;
            let form = reqwest::multipart::Form::new().text("model", model.to_owned()).part("file", part);
            ep.post(http, "/v1/audio/transcriptions").multipart(form)
        }
        Kind::Speech | Kind::Unknown => ep.post(http, "/v1/audio/speech").json(&serde_json::json!({
            "model": model, "input": "x", "voice": "alloy", "response_format": "mp3"
        })),
    };
    let res = req.timeout(Duration::from_secs(15)).send().await.ok()?;
    let status = res.status();
    if status.as_u16() == 429 || status.is_server_error() {
        return None;
    }
    if !status.is_success() {
        return Some(false);
    }
    Some(match kind {
        Kind::Transcription => res.json::<serde_json::Value>().await.is_ok_and(|v| v.get("text").is_some()),
        Kind::Speech => res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("audio/") || ct == "application/octet-stream"),
        _ => true,
    })
}

/// 0.1 s of 8 kHz 16-bit mono silence.
fn silent_wav() -> Vec<u8> {
    let rate: u32 = 8000;
    let data_size: u32 = 800 * 2;
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.resize(wav.len() + data_size as usize, 0);
    wav
}
