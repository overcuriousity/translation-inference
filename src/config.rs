use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::Path;

use figment::Figment;
use figment::providers::{Format, Toml};
use figment::value::Value;
use ipnet::IpNet;
use serde::Deserialize;
use url::Url;

use crate::deployment::Deployment;
use crate::error::Error;

const ENV_PREFIX: &str = "TRANSLATION_INFERENCE__";
/// The keys whose environment values are parsed as TOML-like values: the switches, numbers, lists
/// and tables. Everything else stays the string it is, so a key of digits keeps its leading zeros.
pub const PARSED_ENV_KEYS: [&str; 16] = [
    "allow_plain_http",
    "trusted_proxies",
    "app.byok",
    "app.free_char_limit",
    "app.gated_char_limit",
    "app.upload_max_mb",
    "app.backend.models",
    "app.backend.transcription_models",
    "app.gated.models",
    "app.gated.transcription_models",
    "app.speech.chunk_bytes",
    "app.speech.voices",
    "app.tuning.default_context_size",
    "app.tuning.input_token_ratio",
    "app.tuning.cjk_chars_per_token",
    "app.tuning.latin_chars_per_token",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub bind: SocketAddr,
    pub public_url: Option<Url>,
    pub trusted_proxies: Vec<IpNet>,
    /// Plain http on a non-loopback host: no HSTS and no `Secure` cookie.
    pub allow_plain_http: bool,
    pub app: AppConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 8080)),
            public_url: None,
            trusted_proxies: ["127.0.0.1/32", "::1/128"].iter().filter_map(|s| s.parse().ok()).collect(),
            allow_plain_http: false,
            app: AppConfig::default(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AppConfig {
    pub title: String,
    /// The server's own model endpoint, used by everyone without an access key.
    pub backend: Option<Backend>,
    /// With an access key: anonymous visitors get the free tier and the key unlocks the gated one.
    pub access_key: Option<String>,
    /// A separate endpoint for access-key holders; without one they use `backend`.
    pub gated: Option<Backend>,
    /// Lets visitors connect their own endpoint, to which this server then sends requests.
    pub byok: bool,
    /// Characters per request for the free tier; 0 for no limit.
    pub free_char_limit: usize,
    /// Characters per request for access-key holders; 0 for no limit.
    pub gated_char_limit: usize,
    pub upload_max_mb: u64,
    pub speech: Option<Speech>,
    pub bitvault: Option<Bitvault>,
    pub tuning: Tuning,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            title: "Translate".into(),
            backend: None,
            access_key: None,
            gated: None,
            byok: false,
            free_char_limit: 16_000,
            gated_char_limit: 65_536,
            upload_max_mb: 100,
            speech: None,
            bitvault: None,
            tuning: Tuning::default(),
        }
    }
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("title", &self.title)
            .field("backend", &self.backend)
            .field("access_key", &self.access_key.as_ref().map(|_| "***"))
            .field("gated", &self.gated)
            .field("byok", &self.byok)
            .finish_non_exhaustive()
    }
}

/// An OpenAI-compatible endpoint: `{url}/v1/chat/completions`, `/v1/audio/transcriptions` and
/// `/v1/models`.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backend {
    pub url: Url,
    #[serde(default)]
    pub api_key: Option<String>,
    /// The translation model used when none is chosen.
    pub model: String,
    #[serde(default)]
    pub transcription_model: Option<String>,
    /// The translation models offered; empty asks the endpoint and probes each model.
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub transcription_models: Vec<String>,
}

impl fmt::Debug for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Backend")
            .field("url", &self.url.as_str())
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

/// An OpenAI-compatible `/v1/audio/speech` endpoint.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Speech {
    pub url: Url,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Longest text per synthesis request, in bytes; 0 sends the whole text at once.
    #[serde(default)]
    pub chunk_bytes: usize,
    /// Language code to `voice@model`; only these languages get a Read aloud button.
    pub voices: BTreeMap<String, String>,
}

impl fmt::Debug for Speech {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Speech").field("url", &self.url.as_str()).field("voices", &self.voices).finish_non_exhaustive()
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bitvault {
    /// The BitVault instance's root, such as `https://paste.example.org`.
    pub url: Url,
    #[serde(default)]
    pub uploader_password: Option<String>,
}

impl fmt::Debug for Bitvault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bitvault").field("url", &self.url.as_str()).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Tuning {
    /// Context window in tokens for model ids that do not name one (like `...-32768`).
    pub default_context_size: usize,
    /// Share of the usable tokens given to the input; the rest is left for the output.
    pub input_token_ratio: f64,
    pub cjk_chars_per_token: f64,
    pub latin_chars_per_token: f64,
    /// A translation shorter than this share of its input is logged as possibly summarised.
    pub min_output_ratio: f64,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            default_context_size: 4096,
            input_token_ratio: 0.5,
            cjk_chars_per_token: 1.5,
            latin_chars_per_token: 4.0,
            min_output_ratio: 0.3,
        }
    }
}

/// Loads the config from an optional TOML file, then `TRANSLATION_INFERENCE__SECTION__KEY`
/// variables. `..._FILE=/path` sets the key to the trimmed contents of that file.
pub fn load(file: Option<&Path>) -> Result<Config, Error> {
    load_from(file, std::env::vars())
}

pub fn load_from(file: Option<&Path>, vars: impl Iterator<Item = (String, String)>) -> Result<Config, Error> {
    let mut fig = Figment::new();
    if let Some(path) = file {
        if !path.exists() {
            return Err(Error::invalid(format!("Config file {} does not exist.", path.display())));
        }
        fig = fig.merge(Toml::file(path));
    }
    let mut files = Vec::new();
    for (name, value) in vars {
        let Some(name) = name.strip_prefix(ENV_PREFIX) else { continue };
        let key = |name: &str| name.to_ascii_lowercase().replace("__", ".");
        if let Some(name) = name.strip_suffix("_FILE") {
            let secret = std::fs::read_to_string(&value)
                .map_err(|e| Error::invalid(format!("Cannot read the secret file {value}: {e}.")))?;
            files.push((key(name), secret.trim().to_owned()));
            continue;
        }
        let key = key(name);
        fig = if PARSED_ENV_KEYS.contains(&key.as_str()) {
            fig.merge((key, value.parse::<Value>().unwrap_or_else(|never| match never {})))
        } else {
            fig.merge((key, value))
        };
    }
    // A `_FILE` variable wins over the plain one, whatever order the environment lists them in.
    for pair in files {
        fig = fig.merge(pair);
    }
    let mut cfg: Config = fig.extract().map_err(|e| Error::invalid(format!("Invalid configuration: {e}.")))?;
    cfg.read_secret_files()?;
    Ok(cfg)
}

/// A secret given as `file:///path` is the trimmed contents of that file.
fn read_secret(value: &mut Option<String>) -> Result<(), Error> {
    if let Some(path) = value.as_deref().and_then(|v| v.strip_prefix("file://")) {
        let secret = std::fs::read_to_string(path)
            .map_err(|e| Error::invalid(format!("Cannot read the secret file {path}: {e}.")))?;
        *value = Some(secret.trim().to_owned());
    }
    Ok(())
}

impl Config {
    fn read_secret_files(&mut self) -> Result<(), Error> {
        let a = &mut self.app;
        read_secret(&mut a.access_key)?;
        for b in [&mut a.backend, &mut a.gated].into_iter().flatten() {
            read_secret(&mut b.api_key)?;
        }
        if let Some(s) = &mut a.speech {
            read_secret(&mut s.api_key)?;
        }
        if let Some(b) = &mut a.bitvault {
            read_secret(&mut b.uploader_password)?;
        }
        Ok(())
    }

    /// Stops start-up on a combination that cannot work, naming the key to fix.
    pub fn validate(&self) -> Result<(), Error> {
        let a = &self.app;
        for (name, b) in [("backend", &a.backend), ("gated", &a.gated)] {
            if let Some(b) = b {
                check_endpoint(&format!("app.{name}.url"), &b.url)?;
                if b.model.trim().is_empty() {
                    return Err(Error::invalid(format!("app.{name}.model must name a model.")));
                }
            }
        }
        if a.gated.is_some() && a.access_key.is_none() {
            return Err(Error::invalid("app.gated needs app.access_key, which unlocks it."));
        }
        if a.access_key.as_deref().is_some_and(|k| k.len() < 12) {
            return Err(Error::invalid("app.access_key must be at least 12 characters."));
        }
        if a.backend.is_none() && !a.byok {
            return Err(Error::invalid("Set app.backend, or turn on app.byok so visitors can connect their own."));
        }
        if let Some(s) = &a.speech {
            check_endpoint("app.speech.url", &s.url)?;
            for (lang, v) in &s.voices {
                if parse_voice(v).is_none() {
                    return Err(Error::invalid(format!("app.speech.voices.{lang} must look like voice@model.")));
                }
            }
        }
        if let Some(b) = &a.bitvault {
            check_endpoint("app.bitvault.url", &b.url)?;
        }
        if a.upload_max_mb == 0 || a.upload_max_mb > 4096 {
            return Err(Error::invalid("app.upload_max_mb must be between 1 and 4096."));
        }
        let t = &a.tuning;
        if !(0.1..=0.9).contains(&t.input_token_ratio) {
            return Err(Error::invalid("app.tuning.input_token_ratio must be between 0.1 and 0.9."));
        }
        if t.default_context_size < 1024 || t.cjk_chars_per_token <= 0.0 || t.latin_chars_per_token <= 0.0 {
            return Err(Error::invalid(
                "app.tuning needs default_context_size of at least 1024 and positive chars per token.",
            ));
        }
        Deployment::from_config(self).map(|_| ())
    }

    pub fn gated_mode(&self) -> bool {
        self.app.access_key.is_some()
    }
}

fn check_endpoint(key: &str, url: &Url) -> Result<(), Error> {
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(Error::invalid(format!("{key} must be an http or https URL.")));
    }
    Ok(())
}

/// Splits `voice@model`. The voice may itself contain `@`, the model may not.
pub fn parse_voice(v: &str) -> Option<(&str, &str)> {
    let (voice, model) = v.rsplit_once('@')?;
    let (voice, model) = (voice.trim(), model.trim());
    (!voice.is_empty() && !model.is_empty()).then_some((voice, model))
}

/// A limit of 0 means none.
pub fn limit(n: usize) -> Option<usize> {
    (n > 0).then_some(n)
}
