use askama::Template;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::{Shell, page_with};
use crate::access::Access;
use crate::error::{Error, ErrorNotice};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage<'a> {
    heading: &'a str,
    icon: String,
    message: &'a str,
    back: &'static str,
    back_label: &'static str,
}

fn heading(status: StatusCode) -> &'static str {
    match status {
        StatusCode::NOT_FOUND => "Not found",
        StatusCode::UNAUTHORIZED => "Access key needed",
        StatusCode::FORBIDDEN => "Not allowed",
        StatusCode::TOO_MANY_REQUESTS => "Too many attempts",
        s if s.is_server_error() => "Something went wrong",
        _ => "That did not work",
    }
}

/// Turns the plain response of an [`Error`] on a page into the error page inside the shell.
/// Script requests (`Accept` without HTML) keep the plain sentence, which they show as a notice.
pub async fn render(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let wants_html = req
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));
    let sid = req.extensions().get::<crate::access::Sid>().map(|s| s.0.clone());
    let res = next.run(req).await;
    let Some(notice) = res.extensions().get::<ErrorNotice>().cloned() else { return res };
    if path.starts_with("/api/") || !wants_html {
        return res;
    }
    let session = sid.as_deref().and_then(|id| st.sessions.get(id)).unwrap_or_default();
    let shell = Shell { access: Access::from_session(&st, sid, session) };
    let (back, back_label) =
        if path.starts_with("/settings") { ("/settings", "Back to Settings") } else { ("/", "Back to the start") };
    let page = ErrorPage {
        heading: heading(notice.status),
        icon: super::notice_icon("danger"),
        message: &notice.message,
        back,
        back_label,
    }
    .render()
    .map_err(Error::internal)
    .and_then(|body| page_with(&st, &shell, "", heading(notice.status), body, super::Extras::default()));
    match page {
        Ok(html) => (notice.status, html).into_response(),
        Err(_) => res,
    }
}

pub async fn not_found() -> Error {
    Error::NotFound
}
