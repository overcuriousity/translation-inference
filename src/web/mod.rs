pub mod docs;
pub mod errors;
pub mod pages;
pub mod settings;
pub mod shell;

use askama::Template;
use axum::Router;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::header;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{Html, Response};

use crate::access::{Access, Sid};
use crate::error::Error;
use crate::session::{Session, cookie_id, set_cookie};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().merge(pages::routes()).merge(settings::routes()).merge(docs::routes())
}

/// Gives every page visitor a session, so the browser's API calls are tied to what they unlocked
/// and chose. Only pages start one: API clients authenticate with the key or a session they have.
pub async fn session(State(st): State<AppState>, mut req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if path.starts_with("/api/") || path.starts_with("/assets/") {
        return next.run(req).await;
    }
    let known = cookie_id(req.headers(), &st.dep).filter(|id| st.sessions.get(id).is_some());
    let (id, fresh) = match known {
        Some(id) => (id, false),
        None => (st.sessions.create(Session::default()), true),
    };
    req.extensions_mut().insert(Sid(id.clone()));
    let mut res = next.run(req).await;
    if fresh && !res.headers().contains_key(header::SET_COOKIE) {
        res.headers_mut().append(header::SET_COOKIE, set_cookie(&st.dep, &id));
    }
    res
}

/// What every page's frame needs: the visitor's access, for the navigation.
pub struct Shell {
    pub access: Access,
}

impl FromRequestParts<AppState> for Shell {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Error> {
        let sid = parts.extensions.get::<Sid>().map(|s| s.0.clone());
        let session = sid.as_deref().and_then(|id| st.sessions.get(id)).unwrap_or_default();
        Ok(Self { access: Access::from_session(st, sid, session) })
    }
}

/// Asset URLs a page adds to the shell's own.
#[derive(Debug, Default)]
pub struct Extras {
    pub css: Vec<String>,
    /// ES modules.
    pub js: Vec<String>,
}

pub fn page_with(st: &AppState, shell: &Shell, current: &'static str, title: &str, body: String, extras: Extras) -> Result<Html<String>, Error> {
    let layout = shell::Layout {
        title: title.to_owned(),
        current,
        body,
        files: shell.access.files(),
        extra_css: extras.css,
        extra_js: extras.js,
        cfg: st.cfg.clone(),
    };
    layout.render().map(Html).map_err(Error::internal)
}

pub fn render(t: &impl Template) -> Result<String, Error> {
    t.render().map_err(Error::internal)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSize {
    Row,
    #[default]
    Tool,
    Tab,
}

/// The lucide icon inlined so it takes the surrounding text colour. An unknown name gives an
/// empty string, so a typo shows no icon rather than failing a request.
pub fn icon(name: &str, size: IconSize) -> String {
    let Some(file) = crate::assets::Assets::get(&format!("vendor/oc/icons/{name}.svg")) else {
        debug_assert!(false, "unknown icon {name}");
        return String::new();
    };
    let class = match size {
        IconSize::Row => "oc-icon oc-icon--row",
        IconSize::Tool => "oc-icon",
        IconSize::Tab => "oc-icon oc-icon--tab",
    };
    String::from_utf8_lossy(&file.data).trim_end().replacen("class=\"oc-icon\"", &format!("class=\"{class}\" data-icon=\"{name}\""), 1)
}

/// The status symbol of a notice of `kind`, so no status is told by colour alone.
pub fn notice_icon(kind: &str) -> String {
    let name = match kind {
        "success" => "circle-check",
        "warning" => "triangle-alert",
        "danger" => "circle-x",
        _ => "info",
    };
    icon(name, IconSize::Tool)
}

/// The copy and check icons a copy button flips between.
pub fn copy_icons() -> String {
    format!("{}{}", icon("copy", IconSize::Row), icon("check", IconSize::Row))
}
