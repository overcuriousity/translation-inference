//! `/settings`: unlock with the access key, choose models, connect an own endpoint. Plain forms
//! that post and come back, so they work without scripts.

use std::time::Duration;

use askama::Template;
use axum::extract::{Form, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use super::{Extras, Shell, page_with, render};
use crate::access::{Access, Tier, check_key};
use crate::domain::upstream::{self, Endpoint};
use crate::error::Error;
use crate::security::ClientIp;
use crate::session::{Own, set_cookie};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings", get(show))
        .route("/settings/unlock", post(unlock))
        .route("/settings/lock", post(lock))
        .route("/settings/models", post(models))
        .route("/settings/own", post(connect))
        .route("/settings/own/forget", post(forget))
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    gated_mode: bool,
    tier: Tier,
    byok: bool,
    speech: bool,
    /// The endpoint in use, by host.
    host: String,
    char_limit: Option<usize>,
    models: Vec<String>,
    model: String,
    transcription_models: Vec<String>,
    transcription_model: String,
    own: Option<OwnView>,
    notice: Option<(&'static str, String)>,
    notice_icon: String,
}

struct OwnView {
    url: String,
    transcription_url: String,
    speech_url: String,
}

impl SettingsPage {
    fn tier_label(&self) -> &'static str {
        match self.tier {
            Tier::Personal => "Everything",
            Tier::Free => "Free",
            Tier::Gated => "Unlocked",
            Tier::Own => "Your own endpoint",
        }
    }
}

const DONE: [(&str, &str); 5] = [
    ("unlocked", "Unlocked."),
    ("locked", "Locked. You are back on the free tier."),
    ("models", "Models saved."),
    ("connected", "Connected to your endpoint."),
    ("forgotten", "Your endpoint is forgotten."),
];

#[derive(Deserialize)]
struct Done {
    done: Option<String>,
}

fn page(st: &AppState, access: &Access, notice: Option<(&'static str, String)>) -> Result<String, Error> {
    let m = access.models(st);
    let own = access.session.own.as_ref().map(|o| OwnView {
        url: o.chat.base.clone(),
        transcription_url: o.transcription.as_ref().map(|e| e.base.clone()).unwrap_or_default(),
        speech_url: o.speech.as_ref().map(|e| e.base.clone()).unwrap_or_default(),
    });
    let kind = notice.as_ref().map_or("info", |n| n.0);
    render(&SettingsPage {
        gated_mode: st.cfg.gated_mode(),
        tier: access.tier,
        byok: st.cfg.app.byok,
        speech: st.cfg.app.speech.is_some(),
        host: access.chat(st).map(|e| e.host()).unwrap_or_default(),
        char_limit: access.char_limit(st),
        model: access.model(st, None).unwrap_or_default(),
        transcription_model: access.transcription_model(st, None).unwrap_or_default(),
        models: m.translation,
        transcription_models: m.transcription,
        own,
        notice,
        notice_icon: super::notice_icon(kind),
    })
}

fn respond(st: &AppState, shell: &Shell, status: StatusCode, notice: Option<(&'static str, String)>) -> Response {
    match page(st, &shell.access, notice).and_then(|body| page_with(st, shell, "settings", "Settings", body, Extras::default())) {
        Ok(html) => (status, html).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn show(State(st): State<AppState>, shell: Shell, Query(q): Query<Done>) -> Response {
    let notice = q.done.as_deref().and_then(|d| DONE.iter().find(|(k, _)| *k == d)).map(|(_, m)| ("success", (*m).to_owned()));
    respond(&st, &shell, StatusCode::OK, notice)
}

/// Shows the form again with the error above it.
fn refuse(st: &AppState, shell: &Shell, e: &Error) -> Response {
    e.log();
    respond(st, shell, e.status(), Some(("danger", e.to_string())))
}

fn back(done: &str) -> Redirect {
    Redirect::to(&format!("/settings?done={done}"))
}

fn sid(shell: &Shell) -> Result<&str, Error> {
    shell.access.sid.as_deref().ok_or_else(|| Error::internal("settings without a session"))
}

#[derive(Deserialize)]
struct KeyForm {
    key: String,
}

async fn unlock(State(st): State<AppState>, shell: Shell, ip: Result<ClientIp, Error>, Form(f): Form<KeyForm>) -> Response {
    let ip = ip.ok();
    if !st.cfg.gated_mode() {
        return back("unlocked").into_response();
    }
    let result = check_key(&st, ip, f.key.trim()).and_then(|()| sid(&shell).map(str::to_owned));
    match result {
        Ok(old) => {
            let mut session = shell.access.session.clone();
            session.unlocked = true;
            // A new id, so one seen before unlocking is worth nothing after.
            let id = st.sessions.rotate(&old, session);
            ([(header::SET_COOKIE, set_cookie(&st.dep, &id))], back("unlocked")).into_response()
        }
        Err(e) => refuse(&st, &shell, &e),
    }
}

async fn lock(State(st): State<AppState>, shell: Shell) -> Response {
    match sid(&shell) {
        Ok(id) => {
            st.sessions.update(id, |s| s.unlocked = false);
            back("locked").into_response()
        }
        Err(e) => refuse(&st, &shell, &e),
    }
}

#[derive(Deserialize)]
struct ModelsForm {
    model: Option<String>,
    transcription_model: Option<String>,
}

async fn models(State(st): State<AppState>, shell: Shell, Form(f): Form<ModelsForm>) -> Response {
    let a = &shell.access;
    let chosen = (|| {
        let model = f.model.as_deref().filter(|m| !m.is_empty()).map(|m| a.model(&st, Some(m))).transpose()?;
        let tmodel = f.transcription_model.as_deref().filter(|m| !m.is_empty()).map(|m| a.transcription_model(&st, Some(m))).transpose()?;
        Ok::<_, Error>((model, tmodel, sid(&shell)?.to_owned()))
    })();
    match chosen {
        Ok((model, tmodel, id)) => {
            st.sessions.update(&id, |s| {
                s.model = model;
                s.transcription_model = tmodel;
            });
            back("models").into_response()
        }
        Err(e) => refuse(&st, &shell, &e),
    }
}

#[derive(Deserialize)]
struct OwnForm {
    url: String,
    key: Option<String>,
    transcription_url: Option<String>,
    transcription_key: Option<String>,
    speech_url: Option<String>,
    speech_key: Option<String>,
}

fn endpoint(url: Option<&str>, key: Option<&str>, what: &str) -> Result<Option<Endpoint>, Error> {
    let Some(raw) = url.map(str::trim).filter(|u| !u.is_empty()) else { return Ok(None) };
    match url::Url::parse(raw) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() && u.username().is_empty() => {
            Ok(Some(Endpoint::new(u.as_str(), key)))
        }
        _ => Err(Error::invalid(format!("Enter the {what} address as an http or https URL."))),
    }
}

async fn connect(State(st): State<AppState>, shell: Shell, Form(f): Form<OwnForm>) -> Response {
    if !st.cfg.app.byok {
        return refuse(&st, &shell, &Error::Forbidden);
    }
    let result = async {
        let chat = endpoint(Some(&f.url), f.key.as_deref(), "endpoint")?.ok_or_else(|| Error::invalid("Enter your endpoint's address."))?;
        let transcription = endpoint(f.transcription_url.as_deref(), f.transcription_key.as_deref(), "transcription")?;
        let speech = endpoint(f.speech_url.as_deref(), f.speech_key.as_deref(), "speech")?;
        // The endpoint must answer before it is kept; probing every model can take a while.
        let mut models = tokio::time::timeout(Duration::from_secs(90), upstream::discover(&st.http, &chat, None))
            .await
            .map_err(|_| Error::upstream("Your endpoint took too long to list its models. Try again.", "discover timed out"))??;
        if let Some(t) = &transcription
            && t != &chat
            && let Ok(Ok(m)) = tokio::time::timeout(Duration::from_secs(60), upstream::discover(&st.http, t, None)).await
        {
            models.transcription = m.transcription;
        }
        if models.translation.is_empty() {
            return Err(Error::invalid("Your endpoint offers no model that translates. Check its address."));
        }
        Ok(Own { chat, transcription, speech, models })
    }
    .await;
    match result.and_then(|own| Ok((own, sid(&shell)?.to_owned()))) {
        Ok((own, id)) => {
            st.sessions.update(&id, |s| {
                s.own = Some(own);
                s.model = None;
                s.transcription_model = None;
            });
            back("connected").into_response()
        }
        Err(e) => refuse(&st, &shell, &e),
    }
}

async fn forget(State(st): State<AppState>, shell: Shell) -> Response {
    match sid(&shell) {
        Ok(id) => {
            st.sessions.update(id, |s| {
                s.own = None;
                s.model = None;
                s.transcription_model = None;
            });
            back("forgotten").into_response()
        }
        Err(e) => refuse(&st, &shell, &e),
    }
}
