#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, Response, header};
use axum::routing;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use translation_inference::config::Config;
use translation_inference::state::AppState;

pub const KEY: &str = "correct-horse-battery";
pub const HOST: &str = "localhost:8080";
pub const ORIGIN: &str = "http://localhost:8080";

/// An OpenAI-compatible endpoint that "translates" by wrapping the text in brackets.
pub async fn fake_upstream() -> String {
    async fn chat(axum::Json(req): axum::Json<Value>) -> axum::response::Response {
        use axum::response::IntoResponse;
        let system = req["messages"][0]["content"].as_str().unwrap_or_default();
        let user = req["messages"][1]["content"].as_str().unwrap_or_default().to_owned();
        if system.contains("Identify the language") {
            return axum::Json(json!({"choices": [{"message": {"content": "de"}}]})).into_response();
        }
        let text = user.split("<source_text>\n").last().unwrap().split("\n</source_text>").next().unwrap();
        let out = format!("[{text}]");
        if req["stream"].as_bool() == Some(true) {
            let mut body = String::new();
            for part in out.as_bytes().chunks(3) {
                let delta = json!({"choices": [{"delta": {"content": String::from_utf8_lossy(part)}}]});
                body.push_str(&format!("data: {delta}\n\n"));
            }
            body.push_str("data: [DONE]\n\n");
            return ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response();
        }
        axum::Json(json!({"choices": [{"message": {"content": out}, "finish_reason": "stop"}]})).into_response()
    }
    let app = Router::new()
        .route("/v1/models", routing::get(|| async { axum::Json(json!({"data": [{"id": "fake-llm-32768"}]})) }))
        .route("/v1/chat/completions", routing::post(chat));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

pub fn config(toml: &str) -> Config {
    use figment::providers::{Format, Toml};
    figment::Figment::new().merge(Toml::string(toml)).extract().unwrap()
}

pub struct App {
    pub router: Router,
    pub st: AppState,
}

pub async fn app_with(extra: &str) -> App {
    let upstream = fake_upstream().await;
    let toml = format!(
        "[app]\nbyok = true\n{extra}\n[app.backend]\nurl = \"{upstream}\"\nmodel = \"fake-llm-32768\"\nmodels = [\"fake-llm-32768\"]\n"
    );
    let (router, st) = translation_inference::app::build_with(config(&toml), false, Router::new()).unwrap();
    let router = router.layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    App { router, st }
}

/// A server with an access key: anonymous visitors are on the free tier.
pub async fn gated() -> App {
    app_with(&format!("access_key = \"{KEY}\"\nfree_char_limit = 50")).await
}

pub async fn personal() -> App {
    app_with("").await
}

pub fn get(path: &str) -> Request<Body> {
    Request::get(path).header(header::HOST, HOST).header(header::ACCEPT, "text/html").body(Body::empty()).unwrap()
}

pub fn json_req(path: &str, body: &Value, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::post(path)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, ORIGIN);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::from(body.to_string())).unwrap()
}

pub fn form_req(path: &str, form: &str, cookie: &str) -> Request<Body> {
    Request::post(path)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, cookie)
        .header(header::ACCEPT, "text/html")
        .body(Body::from(form.to_owned()))
        .unwrap()
}

impl App {
    pub async fn send(&self, req: Request<Body>) -> Response<Body> {
        self.router.clone().oneshot(req).await.unwrap()
    }

    /// A page visit's session cookie, as `name=value`.
    pub async fn session(&self) -> String {
        let res = self.send(get("/")).await;
        cookie_of(&res).expect("pages start a session")
    }
}

pub fn cookie_of(res: &Response<Body>) -> Option<String> {
    res.headers().get(header::SET_COOKIE).map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
}

pub async fn text(res: Response<Body>) -> String {
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

pub async fn json(res: Response<Body>) -> Value {
    serde_json::from_str(&text(res).await).unwrap()
}
