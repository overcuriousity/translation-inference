//! Who is asking and what they may use: the tier, the endpoints it reaches and its limits.
//!
//! Without an access key every visitor gets everything (a personal install). With one, anonymous
//! visitors get the free tier, the key unlocks the gated tier, and a visitor's own endpoint (when
//! `app.byok` allows it) takes no limits from the server because it costs the server nothing.

use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use subtle::ConstantTimeEq;

use crate::config::{Backend, limit};
use crate::domain::upstream::{Endpoint, Models};
use crate::error::Error;
use crate::security::ClientIp;
use crate::session::{Session, cookie_id};
use crate::state::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// No access key is configured: everything, without limits.
    Personal,
    Free,
    Gated,
    /// The visitor's own endpoint.
    Own,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Free => "free",
            Self::Gated => "gated",
            Self::Own => "own",
        }
    }
}

/// The session id the web layer made sure of, for this request.
#[derive(Clone, Debug)]
pub struct Sid(pub String);

#[derive(Clone, Debug)]
pub struct Access {
    pub tier: Tier,
    pub sid: Option<String>,
    pub session: Session,
}

pub fn key_matches(st: &AppState, given: &str) -> bool {
    st.cfg.app.access_key.as_deref().is_some_and(|k| bool::from(given.as_bytes().ct_eq(k.as_bytes())))
}

/// Checks an access key against the per-address budget for wrong keys.
pub fn check_key(st: &AppState, ip: Option<ClientIp>, given: &str) -> Result<(), Error> {
    if let Some(ClientIp(ip)) = ip {
        st.key_failures.take(ip)?;
    }
    if key_matches(st, given) {
        if let Some(ClientIp(ip)) = ip {
            st.key_failures.refund(ip);
        }
        Ok(())
    } else {
        Err(Error::WrongKey)
    }
}

impl Access {
    pub fn from_session(st: &AppState, sid: Option<String>, session: Session) -> Self {
        let tier = if session.own.is_some() {
            Tier::Own
        } else if !st.cfg.gated_mode() {
            Tier::Personal
        } else if session.unlocked {
            Tier::Gated
        } else {
            Tier::Free
        };
        Self { tier, sid, session }
    }

    /// The server's backend this tier uses; `None` for a visitor's own endpoint.
    pub fn backend<'a>(&self, st: &'a AppState) -> Option<&'a Backend> {
        let a = &st.cfg.app;
        match self.tier {
            Tier::Own => None,
            Tier::Gated => a.gated.as_ref().or(a.backend.as_ref()),
            Tier::Personal | Tier::Free => a.backend.as_ref(),
        }
    }

    fn server_endpoint(b: &Backend) -> Endpoint {
        Endpoint::new(b.url.as_str(), b.api_key.as_deref())
    }

    pub fn chat(&self, st: &AppState) -> Result<Endpoint, Error> {
        if let Some(own) = &self.session.own {
            return Ok(own.chat.clone());
        }
        self.backend(st)
            .map(Self::server_endpoint)
            .ok_or(Error::NotConfigured("translation endpoint; connect your own in Settings"))
    }

    pub fn transcription(&self, st: &AppState) -> Result<Endpoint, Error> {
        if !st.ffmpeg {
            return Err(Error::NotConfigured("ffmpeg, which transcription needs"));
        }
        if let Some(own) = &self.session.own {
            return Ok(own.transcription.clone().unwrap_or_else(|| own.chat.clone()));
        }
        self.chat(st)
    }

    pub fn speech(&self, st: &AppState) -> Result<Endpoint, Error> {
        if let Some(ep) = self.session.own.as_ref().and_then(|o| o.speech.clone()) {
            return Ok(ep);
        }
        let s = st.cfg.app.speech.as_ref().ok_or(Error::NotConfigured("speech endpoint"))?;
        Ok(Endpoint::new(s.url.as_str(), s.api_key.as_deref()))
    }

    pub fn char_limit(&self, st: &AppState) -> Option<usize> {
        match self.tier {
            Tier::Free => limit(st.cfg.app.free_char_limit),
            Tier::Gated => limit(st.cfg.app.gated_char_limit),
            Tier::Personal | Tier::Own => None,
        }
    }

    pub fn check_len(&self, st: &AppState, text: &str) -> Result<(), Error> {
        match self.char_limit(st) {
            Some(limit) => {
                let len = text.chars().count();
                if len > limit { Err(Error::TooLong { len, limit }) } else { Ok(()) }
            }
            None => Ok(()),
        }
    }

    /// Subtitle files; the free tier keeps to typed text.
    pub fn files(&self) -> bool {
        self.tier != Tier::Free
    }

    /// The models offered to this visitor, the default translation model first.
    pub fn models(&self, st: &AppState) -> Models {
        if let Some(own) = &self.session.own {
            return own.models.clone();
        }
        let Some(b) = self.backend(st) else { return Models::default() };
        let mut m = if b.models.is_empty() && b.transcription_models.is_empty() {
            st.catalog.models(&Self::server_endpoint(b)).unwrap_or_default()
        } else {
            Models { translation: b.models.clone(), transcription: b.transcription_models.clone() }
        };
        if !m.translation.contains(&b.model) {
            m.translation.insert(0, b.model.clone());
        }
        if let Some(t) = &b.transcription_model
            && !m.transcription.contains(t)
        {
            m.transcription.insert(0, t.clone());
        }
        m
    }

    /// The translation model to use: the requested one, else the visitor's choice, else the
    /// default. A model the server does not offer is refused.
    pub fn model(&self, st: &AppState, requested: Option<&str>) -> Result<String, Error> {
        let models = self.models(st);
        let wanted = requested.or(self.session.model.as_deref()).map(str::trim).filter(|m| !m.is_empty());
        match wanted {
            Some(m) if models.translation.iter().any(|x| x == m) => Ok(m.to_owned()),
            // An own endpoint that lists no models still takes the name it was given.
            Some(m) if self.tier == Tier::Own && models.translation.is_empty() => Ok(m.to_owned()),
            Some(_) if requested.is_some() => {
                Err(Error::invalid("That model is not offered here. Choose one from /api/v1/models."))
            }
            _ => models
                .translation
                .first()
                .cloned()
                .ok_or_else(|| Error::invalid("Choose a translation model in Settings.")),
        }
    }

    pub fn transcription_model(&self, st: &AppState, requested: Option<&str>) -> Result<String, Error> {
        let models = self.models(st);
        let wanted = requested.or(self.session.transcription_model.as_deref()).map(str::trim).filter(|m| !m.is_empty());
        match wanted {
            Some(m) if models.transcription.iter().any(|x| x == m) => Ok(m.to_owned()),
            Some(m) if self.tier == Tier::Own && models.transcription.is_empty() => Ok(m.to_owned()),
            Some(_) if requested.is_some() => Err(Error::invalid("That transcription model is not offered here.")),
            _ => models.transcription.first().cloned().ok_or(Error::NotConfigured("transcription model")),
        }
    }

    /// Speech in `lang`: the voice and model configured for it.
    pub fn voice<'a>(&self, st: &'a AppState, lang: &str) -> Result<(&'a str, &'a str), Error> {
        let s = st.cfg.app.speech.as_ref().ok_or(Error::NotConfigured("speech endpoint"))?;
        s.voices
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(lang))
            .and_then(|(_, v)| crate::config::parse_voice(v))
            .ok_or_else(|| Error::invalid("There is no voice for that language."))
    }
}

impl FromRequestParts<AppState> for Access {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let bearer = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim);
        if let Some(given) = bearer
            && st.cfg.gated_mode()
        {
            check_key(st, parts.extensions.get::<ClientIp>().copied(), given)?;
            return Ok(Self {
                tier: Tier::Gated,
                sid: None,
                session: Session { unlocked: true, ..Session::default() },
            });
        }
        let sid = parts.extensions.get::<Sid>().map(|s| s.0.clone()).or_else(|| cookie_id(&parts.headers, &st.dep));
        if let Some(id) = sid
            && let Some(session) = st.sessions.get(&id)
        {
            return Ok(Self::from_session(st, Some(id), session));
        }
        if st.cfg.gated_mode() {
            return Err(Error::Unauthorized);
        }
        Ok(Self::from_session(st, None, Session::default()))
    }
}
