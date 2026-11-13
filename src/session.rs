//! Visitor sessions, kept in memory only: what a visitor unlocked, connected and chose lasts until
//! they reset it, stay away for a day, or the server restarts.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, HeaderValue, header};

use crate::deployment::Deployment;
use crate::domain::upstream::{Endpoint, Models};

const IDLE: Duration = Duration::from_secs(24 * 3600);
const MAX_SESSIONS: usize = 10_000;

/// A visitor's own endpoints, used instead of the server's.
#[derive(Clone, Debug)]
pub struct Own {
    pub chat: Endpoint,
    pub transcription: Option<Endpoint>,
    pub speech: Option<Endpoint>,
    pub models: Models,
}

#[derive(Clone, Debug, Default)]
pub struct Session {
    /// The visitor entered the access key.
    pub unlocked: bool,
    pub own: Option<Own>,
    pub model: Option<String>,
    pub transcription_model: Option<String>,
}

struct Entry {
    session: Session,
    seen: Instant,
}

#[derive(Default)]
pub struct Sessions {
    map: Mutex<HashMap<String, Entry>>,
}

fn new_id() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

impl Sessions {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self, id: &str) -> Option<Session> {
        let mut map = self.lock();
        let now = Instant::now();
        let entry = map.get_mut(id).filter(|e| now.duration_since(e.seen) < IDLE)?;
        entry.seen = now;
        Some(entry.session.clone())
    }

    /// Starts a session and returns its id. When full, idle sessions go first, then the one
    /// unused the longest.
    pub fn create(&self, session: Session) -> String {
        let id = new_id();
        let mut map = self.lock();
        let now = Instant::now();
        if map.len() >= MAX_SESSIONS {
            map.retain(|_, e| now.duration_since(e.seen) < IDLE);
        }
        if map.len() >= MAX_SESSIONS
            && let Some(oldest) = map.iter().min_by_key(|(_, e)| e.seen).map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        }
        map.insert(id.clone(), Entry { session, seen: now });
        id
    }

    /// Changes a session; false when it no longer exists.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Session)) -> bool {
        match self.lock().get_mut(id) {
            Some(e) => {
                f(&mut e.session);
                e.seen = Instant::now();
                true
            }
            None => false,
        }
    }

    /// Replaces a session under a new id, so an id seen before unlocking is worth nothing after.
    pub fn rotate(&self, old: &str, session: Session) -> String {
        self.lock().remove(old);
        self.create(session)
    }

    pub fn remove(&self, id: &str) {
        self.lock().remove(id);
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The session id the request's cookie names, if any.
pub fn cookie_id(headers: &HeaderMap, dep: &Deployment) -> Option<String> {
    let name = dep.cookie_name();
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// A cookie for the browser session only: no Max-Age, so closing the browser ends it.
pub fn set_cookie(dep: &Deployment, id: &str) -> HeaderValue {
    let secure = if dep.cookie_secure() { "; Secure" } else { "" };
    let value = format!("{}={id}; Path=/; HttpOnly; SameSite=Lax{secure}", dep.cookie_name());
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static(""))
}
