use std::sync::Arc;

use reqwest::Client;

use crate::config::Config;
use crate::deployment::Deployment;
use crate::domain::upstream::Catalog;
use crate::ratelimit::RateLimiter;
use crate::session::Sessions;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub dep: Arc<Deployment>,
    pub http: Client,
    pub catalog: Arc<Catalog>,
    pub sessions: Arc<Sessions>,
    /// Wrong access keys, per client address.
    pub key_failures: Arc<RateLimiter>,
    /// ffmpeg was found at start-up; without it there is no transcription.
    pub ffmpeg: bool,
}
