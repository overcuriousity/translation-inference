#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use axum::http::{StatusCode, header};
use support::*;

const PAGES: [&str; 4] = ["/", "/talk", "/settings", "/docs"];

#[tokio::test]
async fn pages_carry_the_strict_policy_and_the_security_headers() {
    let app = personal().await;
    for path in PAGES {
        let res = app.send(get(path)).await;
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        let h = res.headers();
        let csp = h.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap();
        assert!(csp.starts_with("default-src 'none'; script-src 'self'; style-src 'self'"));
        assert!(!csp.contains("unsafe"));
        assert_eq!(h.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
        assert_eq!(h.get(header::REFERRER_POLICY).unwrap(), "no-referrer");
        assert_eq!(h.get("cross-origin-opener-policy").unwrap(), "same-origin");
        assert_eq!(h.get(header::CACHE_CONTROL).unwrap(), "no-store");
    }
}

#[tokio::test]
async fn rendered_pages_have_no_inline_styles_scripts_or_handlers() {
    let app = personal().await;
    let handler = regex::Regex::new(r#"\son[a-z]+\s*="#).unwrap();
    let inline_script = regex::Regex::new(r#"<script(?:\s[^>]*)?>\s*[^<\s]"#).unwrap();
    for path in PAGES.iter().chain(&["/files", "/nothing-here"]) {
        let html = text(app.send(get(path)).await).await;
        assert!(!html.contains("style="), "{path} has a style attribute");
        assert!(!html.contains("<style"), "{path} has a style element");
        assert!(!handler.is_match(&html), "{path} has an event handler attribute");
        assert!(!inline_script.is_match(&html), "{path} has an inline script");
        assert!(
            !html.contains("http://") || !html.contains("<script src=\"http"),
            "{path} loads a script from elsewhere"
        );
    }
}

#[tokio::test]
async fn the_session_cookie_is_host_only_http_only_and_secure_on_loopback() {
    let app = personal().await;
    let res = app.send(get("/")).await;
    let set = res.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(set.starts_with("__Host-translation-inference-session="));
    for flag in ["HttpOnly", "SameSite=Lax", "Secure", "Path=/"] {
        assert!(set.contains(flag), "{flag} missing in {set}");
    }
}

#[tokio::test]
async fn an_unknown_host_name_is_refused() {
    let app = personal().await;
    let mut req = get("/");
    req.headers_mut().insert(header::HOST, "evil.example".parse().unwrap());
    assert_eq!(app.send(req).await.status(), StatusCode::MISDIRECTED_REQUEST);
}

#[tokio::test]
async fn a_cross_site_post_is_blocked_with_the_json_error() {
    let app = personal().await;
    let mut req = json_req("/api/v1/translate", &serde_json::json!({"text": "a", "target": "en"}), None);
    req.headers_mut().insert(header::ORIGIN, "https://evil.example".parse().unwrap());
    let res = app.send(req).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert!(res.headers().contains_key(header::X_CONTENT_TYPE_OPTIONS));
    assert_eq!(json(res).await["error"]["code"], "FORBIDDEN");
}

#[tokio::test]
async fn unlocking_with_the_key_starts_a_new_session_and_shows_the_files_tab() {
    let app = gated().await;
    let cookie = app.session().await;
    let page = text(app.send(get("/").with(&cookie)).await).await;
    assert!(!page.contains("href=\"/files\""), "the free tier has no Files tab");

    let res = app.send(form_req("/settings/unlock", "key=wrong-key-here", &cookie)).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(text(res).await.contains("That access key is not right."));

    let res = app.send(form_req("/settings/unlock", &format!("key={KEY}"), &cookie)).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let fresh = cookie_of(&res).expect("a new session id");
    assert_ne!(fresh, cookie);
    let page = text(app.send(get("/").with(&fresh)).await).await;
    assert!(page.contains("href=\"/files\""));
    assert_eq!(app.send(get("/files").with(&fresh)).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn markdown_is_rendered_without_raw_html() {
    let app = personal().await;
    let cookie = app.session().await;
    let res = app
        .send(json_req("/markdown", &serde_json::json!({"text": "**a** <img src=x onerror=1>"}), Some(&cookie)))
        .await;
    let html = json(res).await["html"].as_str().unwrap().to_owned();
    assert!(html.contains("<strong>a</strong>"));
    assert!(!html.contains("<img"));
}

#[tokio::test]
async fn a_missing_page_renders_the_error_page() {
    let app = personal().await;
    let res = app.send(get("/nothing-here")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(text(res).await.contains("Nothing was found at this address."));
}

trait With {
    fn with(self, cookie: &str) -> Self;
}

impl With for axum::http::Request<axum::body::Body> {
    fn with(mut self, cookie: &str) -> Self {
        self.headers_mut().insert(header::COOKIE, cookie.parse().unwrap());
        self
    }
}
