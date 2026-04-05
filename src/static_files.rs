use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode, Uri},
    response::Response,
};
use rust_embed::Embed;
use std::sync::Arc;

use crate::routes::translate::get_session_id;
use crate::{AppState, SessionCredentials, SessionTier};

#[derive(Embed)]
#[folder = "static/"]
struct Assets;

/// Serve the main page, issuing an anonymous Free-tier session cookie when the
/// server is in gated mode and the browser doesn't already have a valid session.
pub async fn serve_index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    // Only set up a free-tier session when:
    //  - the gated tier is configured (REST API requires auth), AND
    //  - the server has its own backend credentials to use as the free tier.
    let cookie = if state.config.is_gated_configured() && state.config.is_configured() {
        let has_valid_session = get_session_id(&headers)
            .map(|sid| state.sessions.read().unwrap().contains_key(&sid))
            .unwrap_or(false);

        if !has_valid_session {
            let sid = uuid::Uuid::new_v4().to_string();
            {
                let mut sessions = state.sessions.write().unwrap();
                // Evict oldest entry if the store is full.
                if sessions.len() >= 1000 {
                    if let Some(old) = sessions.keys().next().cloned() {
                        sessions.remove(&old);
                    }
                }
                sessions.insert(sid.clone(), SessionCredentials {
                    endpoint: String::new(),
                    api_key: String::new(),
                    tier: SessionTier::Free,
                });
            }
            Some(crate::routes::config::make_session_cookie(&sid))
        } else {
            None
        }
    } else {
        None
    };

    match Assets::get("index.html") {
        Some(content) => {
            let mut builder = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8");
            if let Some(c) = cookie {
                builder = builder.header(header::SET_COOKIE, c);
            }
            builder.body(Body::from(content.data.into_owned())).unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap(),
    }
}

pub async fn serve_static(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = path.strip_prefix("static/").unwrap_or(path);
    let path = if path.is_empty() || path == "index.html" {
        "index.html"
    } else {
        path
    };

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path)
                .first_or(mime_guess::mime::APPLICATION_OCTET_STREAM);

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap(),
    }
}

pub async fn get_openapi_spec() -> Response {
    match Assets::get("openapi.yaml") {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/yaml")
            .body(Body::from(content.data.into_owned()))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("openapi.yaml not found"))
            .unwrap(),
    }
}

pub async fn get_swagger_docs() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(include_str!("../static/swagger.html")))
        .unwrap()
}
