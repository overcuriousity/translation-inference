//! The REST API under `/api/v1`. Every handler is registered through utoipa-axum, so the spec
//! cannot drift from the routes. The web pages use the same routes from the browser.

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Multipart, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::access::Access;
use crate::domain::translate::{self, Job, Piece};
use crate::domain::{bitvault, detect, languages, speech, subtitle, transcribe};
use crate::error::{Error, ErrorNotice};
use crate::ratelimit::{self, RateLimiter};
use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "translation-inference API",
        description = "Translate text, subtitles and speech through the server's model endpoints.

Without an access key on the server every route is open. With one, send it as \
`Authorization: Bearer <key>`; browsers use their session cookie instead. Each client address may \
make 120 requests a minute with a burst of 30, and 10 wrong keys a minute with a burst of 5.

Errors are JSON `{\"error\": {\"code\": \"...\", \"message\": \"...\"}}`, where `message` is one \
sentence for a person. Codes: `UNAUTHORIZED` and `WRONG_KEY` (401), `FORBIDDEN` (403), \
`NOT_FOUND` (404), `TOO_LONG` and `TOO_LARGE` (413), `INVALID` (422), `TOO_MANY_ATTEMPTS` (429), \
`UPSTREAM` (502, the model endpoint failed), `NOT_CONFIGURED` (503) and `INTERNAL` (500)."
    ),
    tags(
        (name = "translate", description = "Text and subtitles."),
        (name = "speech", description = "Speech to text and text to speech."),
        (name = "server", description = "What the server offers."),
    ),
)]
struct Doc;

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Stable and machine-readable, e.g. `TOO_LONG`.
    pub code: String,
    /// One sentence for a person.
    pub message: String,
}

fn error_json(code: &str, message: &str, status: StatusCode) -> Response {
    let body = ErrorBody { error: ErrorDetail { code: code.to_owned(), message: message.to_owned() } };
    (status, Json(body)).into_response()
}

/// Under `/api/`, every error is the JSON error body: handler errors keep their code, and the
/// plain-text refusals of the layers (limits, timeouts, origin) get one by status.
pub async fn json_errors(req: Request, next: Next) -> Response {
    let api = req.uri().path().starts_with("/api/");
    let res = next.run(req).await;
    let status = res.status();
    if !api || !(status.is_client_error() || status.is_server_error()) {
        return res;
    }
    if let Some(n) = res.extensions().get::<ErrorNotice>() {
        return error_json(n.code, &n.message, n.status);
    }
    let is_json =
        res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("application/json"));
    if is_json {
        return res;
    }
    let (code, message) = match status {
        StatusCode::PAYLOAD_TOO_LARGE => ("TOO_LARGE", "That request is too large."),
        StatusCode::TOO_MANY_REQUESTS => ("TOO_MANY_ATTEMPTS", "Too many requests. Wait a minute and try again."),
        StatusCode::REQUEST_TIMEOUT => ("TIMEOUT", "The request took too long. Try again with less text."),
        StatusCode::FORBIDDEN => ("FORBIDDEN", "This request came from another site and was blocked."),
        StatusCode::NOT_FOUND => ("NOT_FOUND", "There is no such API route. See /docs."),
        StatusCode::METHOD_NOT_ALLOWED => ("INVALID", "That method is not allowed here. See /docs."),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ("INVALID", "Send the body as application/json."),
        s if s.is_client_error() => ("INVALID", "The request could not be read. Check it against /docs."),
        _ => ("INTERNAL", "Something went wrong. Try again later."),
    };
    error_json(code, message, status)
}

/// `Json`, with its rejections as API errors.
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = Error;

    async fn from_request(req: Request, st: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, st).await {
            Ok(Json(v)) => Ok(Self(v)),
            Err(JsonRejection::MissingJsonContentType(_)) => Err(Error::invalid("Send the body as application/json.")),
            Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => Err(Error::TooLarge),
            Err(_) => Err(Error::invalid("The JSON body does not match this endpoint. Check it against /docs.")),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Server

#[derive(Serialize, ToSchema)]
pub struct Health {
    #[schema(example = "ok")]
    pub status: &'static str,
    pub version: &'static str,
}

#[utoipa::path(get, path = "/api/v1/health", tag = "server", responses((status = 200, body = Health)))]
async fn health() -> Json<Health> {
    Json(Health { status: "ok", version: env!("CARGO_PKG_VERSION") })
}

#[derive(Serialize, ToSchema)]
pub struct Language {
    #[schema(example = "de")]
    pub code: &'static str,
    #[schema(example = "German")]
    pub name: &'static str,
}

/// The languages offered by name. `target` also takes any other language name as free text.
#[utoipa::path(get, path = "/api/v1/languages", tag = "server", responses((status = 200, body = [Language])))]
async fn list_languages() -> Json<Vec<Language>> {
    Json(languages::LANGUAGES.iter().map(|(code, name)| Language { code, name }).collect())
}

#[derive(Serialize, ToSchema)]
pub struct ModelsResponse {
    /// Translation models; the first is the default.
    pub translation: Vec<String>,
    /// Transcription models; the first is the default.
    pub transcription: Vec<String>,
    /// The tier the request is served in: `personal`, `free`, `gated` or `own`.
    pub tier: &'static str,
    /// Characters per request, when limited.
    pub char_limit: Option<usize>,
    /// Languages that can be read aloud.
    pub speech_languages: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/models",
    tag = "server",
    responses((status = 200, body = ModelsResponse), (status = 401, body = ErrorBody))
)]
async fn list_models(State(st): State<AppState>, access: Access) -> Json<ModelsResponse> {
    let m = access.models(&st);
    Json(ModelsResponse {
        translation: m.translation,
        transcription: m.transcription,
        tier: access.tier.as_str(),
        char_limit: access.char_limit(&st),
        speech_languages: st.cfg.app.speech.as_ref().map(|s| s.voices.keys().cloned().collect()).unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------------------------
// Translate

#[derive(Deserialize, ToSchema)]
pub struct TranslateRequest {
    pub text: String,
    /// A code from `/api/v1/languages`, or a language name such as `Low German`.
    #[schema(example = "de")]
    pub target: String,
    /// The source language, or `auto` (the default) to let the model tell.
    pub source: Option<String>,
    /// A hint on the subject or register, at most 300 characters, such as `medical`.
    pub context: Option<String>,
    /// One of `/api/v1/models`; the default otherwise.
    pub model: Option<String>,
}

impl TranslateRequest {
    fn job(self, st: &AppState, access: &Access) -> Result<Job, Error> {
        access.check_len(st, &self.text)?;
        let target = languages::target(&self.target)
            .ok_or_else(|| Error::invalid("Choose a target language: a code from /api/v1/languages or a language name."))?;
        let source = match self.source.as_deref().map(str::trim) {
            None | Some("") | Some("auto") => "auto".to_owned(),
            Some(s) => languages::target(s).ok_or_else(|| Error::invalid("That source language is not one I know."))?,
        };
        Ok(Job { model: access.model(st, self.model.as_deref())?, source, target, text: self.text, context: self.context })
    }
}

#[derive(Serialize, ToSchema)]
pub struct TranslateResponse {
    pub text: String,
    /// How many pieces the text was translated in, to fit the model's context window.
    pub chunks: usize,
}

/// Translate text, waiting for the whole translation.
#[utoipa::path(
    post,
    path = "/api/v1/translate",
    tag = "translate",
    request_body = TranslateRequest,
    responses(
        (status = 200, body = TranslateResponse),
        (status = 413, description = "Longer than the tier allows (`TOO_LONG`).", body = ErrorBody),
        (status = 502, description = "The model endpoint failed (`UPSTREAM`).", body = ErrorBody),
    )
)]
async fn translate_text(
    State(st): State<AppState>,
    access: Access,
    ApiJson(req): ApiJson<TranslateRequest>,
) -> Result<Json<TranslateResponse>, Error> {
    let job = req.job(&st, &access)?;
    if job.text.trim().is_empty() {
        return Ok(Json(TranslateResponse { text: String::new(), chunks: 0 }));
    }
    let (text, chunks) = translate::translate(&st.http, &access.chat(&st)?, &job, &st.cfg.app.tuning).await?;
    Ok(Json(TranslateResponse { text, chunks }))
}

fn event(name: &str, data: &impl Serialize) -> Event {
    Event::default().event(name).data(serde_json::to_string(data).unwrap_or_else(|_| "{}".into()))
}

fn error_event(e: &Error) -> Event {
    e.log();
    event("error", &serde_json::json!({ "code": e.code(), "message": e.to_string() }))
}

fn sse(stream: impl Stream<Item = Event> + Send + 'static) -> Response {
    let mut res = Sse::new(stream.map(Ok::<_, Infallible>)).keep_alive(KeepAlive::default()).into_response();
    // nginx and similar proxies would otherwise hold the stream back.
    res.headers_mut().insert("x-accel-buffering", HeaderValue::from_static("no"));
    res
}

/// Translate text as server-sent events, token by token.
///
/// Events: `chunk` `{"index": 1, "total": 2}` when a piece begins, `text` `{"text": "..."}` for
/// each part of the translation, `truncated` `{}` when the model stopped at its output limit,
/// then `done` `{"chunks": 2}`, or `error` `{"code": "...", "message": "..."}`.
#[utoipa::path(
    post,
    path = "/api/v1/translate/stream",
    tag = "translate",
    request_body = TranslateRequest,
    responses(
        (status = 200, description = "An event stream.", content_type = "text/event-stream"),
        (status = 413, description = "Longer than the tier allows (`TOO_LONG`).", body = ErrorBody),
    )
)]
async fn translate_stream(State(st): State<AppState>, access: Access, ApiJson(req): ApiJson<TranslateRequest>) -> Result<Response, Error> {
    let job = req.job(&st, &access)?;
    let ep = access.chat(&st)?;
    let pieces = translate::translate_stream(st.http.clone(), ep, job, st.cfg.app.tuning.clone());
    let stream = async_stream::stream! {
        let mut chunks = 0;
        futures_util::pin_mut!(pieces);
        while let Some(piece) = pieces.next().await {
            match piece {
                Ok(Piece::Chunk { index, total }) => {
                    chunks = total;
                    yield event("chunk", &serde_json::json!({ "index": index, "total": total }));
                }
                Ok(Piece::Text(text)) => yield event("text", &serde_json::json!({ "text": text })),
                Ok(Piece::Truncated) => yield event("truncated", &serde_json::json!({})),
                Err(e) => {
                    yield error_event(&e);
                    return;
                }
            }
        }
        yield event("done", &serde_json::json!({ "chunks": chunks }));
    };
    Ok(sse(stream))
}

#[derive(Serialize, ToSchema)]
pub struct Paragraph {
    pub source: String,
    pub translation: String,
}

/// Translate text paragraph by paragraph, for a side-by-side view.
///
/// Paragraphs are separated by blank lines. The whole text goes to the model in one request, so
/// it must fit the model's context window.
#[utoipa::path(
    post,
    path = "/api/v1/translate/paragraphs",
    tag = "translate",
    request_body = TranslateRequest,
    responses(
        (status = 200, body = [Paragraph]),
        (status = 422, description = "Too long for one request (`INVALID`).", body = ErrorBody),
    )
)]
async fn translate_paragraphs(
    State(st): State<AppState>,
    access: Access,
    ApiJson(req): ApiJson<TranslateRequest>,
) -> Result<Json<Vec<Paragraph>>, Error> {
    let job = req.job(&st, &access)?;
    let pairs = translate::paragraphs(&st.http, &access.chat(&st)?, &job, &st.cfg.app.tuning).await?;
    Ok(Json(pairs.into_iter().map(|(source, translation)| Paragraph { source, translation }).collect()))
}

#[derive(Deserialize, ToSchema)]
pub struct DetectRequest {
    pub text: String,
}

#[derive(Serialize, ToSchema)]
pub struct DetectResponse {
    #[schema(example = "de")]
    pub language: String,
}

/// Detect the language of a text from its first 500 characters.
#[utoipa::path(
    post,
    path = "/api/v1/detect",
    tag = "translate",
    request_body = DetectRequest,
    responses((status = 200, body = DetectResponse), (status = 502, body = ErrorBody))
)]
async fn detect_language(State(st): State<AppState>, access: Access, ApiJson(req): ApiJson<DetectRequest>) -> Result<Json<DetectResponse>, Error> {
    let model = access.model(&st, None)?;
    let language = detect::detect(&st.http, &access.chat(&st)?, &model, &req.text).await?;
    Ok(Json(DetectResponse { language }))
}

// ---------------------------------------------------------------------------------------------
// Files

struct Upload {
    name: String,
    file: NamedTempFile,
    len: usize,
}

/// Reads a multipart body: text fields into the map, the `file` part to a temporary file.
async fn read_form(mut mp: Multipart, max: usize) -> Result<(std::collections::HashMap<String, String>, Option<Upload>), Error> {
    let bad = |_| Error::invalid("The upload could not be read. Try again.");
    let mut fields = std::collections::HashMap::new();
    let mut upload = None;
    while let Some(mut field) = mp.next_field().await.map_err(bad)? {
        let name = field.name().unwrap_or_default().to_owned();
        if name == "file" {
            let filename = field.file_name().unwrap_or("upload").to_owned();
            let ext = std::path::Path::new(&filename).extension().and_then(|e| e.to_str()).unwrap_or("bin").to_ascii_lowercase();
            let ext: String = ext.chars().filter(char::is_ascii_alphanumeric).take(8).collect();
            let file = tempfile::Builder::new().suffix(&format!(".{ext}")).tempfile()?;
            let mut out = tokio::fs::File::from_std(file.reopen()?);
            let mut len = 0;
            while let Some(chunk) = field.chunk().await.map_err(|e| {
                if e.status() == StatusCode::PAYLOAD_TOO_LARGE { Error::TooLarge } else { Error::invalid("The upload broke off. Try again.") }
            })? {
                len += chunk.len();
                if len > max {
                    return Err(Error::TooLarge);
                }
                out.write_all(&chunk).await?;
            }
            out.flush().await?;
            upload = Some(Upload { name: filename, file, len });
        } else if !name.is_empty() {
            let text = field.text().await.map_err(bad)?;
            fields.insert(name, text);
        }
    }
    Ok((fields, upload))
}

/// Only describes the body for the spec.
#[derive(ToSchema)]
#[allow(dead_code)]
struct TranscribeForm {
    /// An audio or video file.
    #[schema(format = Binary)]
    file: String,
    /// A two-letter language hint; absent lets the model tell.
    language: Option<String>,
    model: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct TranscribeResponse {
    pub text: String,
}

/// Transcribe an audio or video file.
///
/// Video is reduced to its audio first; recordings longer than about 12 minutes are sent in
/// ten-minute segments.
#[utoipa::path(
    post,
    path = "/api/v1/transcribe",
    tag = "speech",
    request_body(content = TranscribeForm, content_type = "multipart/form-data"),
    responses(
        (status = 200, body = TranscribeResponse),
        (status = 413, description = "The file is too large (`TOO_LARGE`).", body = ErrorBody),
        (status = 422, description = "Not an audio or video file (`INVALID`).", body = ErrorBody),
    )
)]
async fn transcribe_file(State(st): State<AppState>, access: Access, mp: Multipart) -> Result<Json<TranscribeResponse>, Error> {
    let ep = access.transcription(&st)?;
    let (fields, upload) = read_form(mp, upload_limit(&st)).await?;
    let upload = upload.ok_or_else(|| Error::invalid("Attach the audio or video as the file part."))?;
    // Browser recordings arrive as "blob" without an extension; ffmpeg reads them by content.
    if !transcribe::is_media(&upload.name) && upload.name.contains('.') {
        return Err(Error::invalid("That is not an audio or video file. Choose a different file."));
    }
    let model = access.transcription_model(&st, fields.get("model").map(String::as_str))?;
    let language = fields.get("language").map(|l| l.trim()).filter(|l| !l.is_empty() && *l != "auto");
    let text = transcribe::transcribe(&st.http, &ep, &model, language, &upload.file).await?;
    Ok(Json(TranscribeResponse { text }))
}

#[derive(ToSchema)]
#[allow(dead_code)]
struct SubtitleForm {
    /// An `.srt` or `.vtt` file in UTF-8.
    #[schema(format = Binary)]
    file: String,
    target: String,
    source: Option<String>,
    context: Option<String>,
    model: Option<String>,
}

/// Translate a subtitle file cue by cue, as server-sent events.
///
/// Events: `progress` `{"done": 3, "total": 120}` after each cue, then `done`
/// `{"filename": "film.de.srt", "content": "..."}`, or `error` `{"code": "...", "message": "..."}`.
/// Not available in the free tier.
#[utoipa::path(
    post,
    path = "/api/v1/subtitles",
    tag = "translate",
    request_body(content = SubtitleForm, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "An event stream.", content_type = "text/event-stream"),
        (status = 403, description = "The free tier has no file translation (`FORBIDDEN`).", body = ErrorBody),
        (status = 422, description = "Not a subtitle file (`INVALID`).", body = ErrorBody),
    )
)]
async fn translate_subtitles(State(st): State<AppState>, access: Access, mp: Multipart) -> Result<Response, Error> {
    if !access.files() {
        return Err(Error::Forbidden);
    }
    let (fields, upload) = read_form(mp, 8 << 20).await?;
    let upload = upload.ok_or_else(|| Error::invalid("Attach the subtitles as the file part."))?;
    let path = std::path::Path::new(&upload.name);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    if ext != "srt" && ext != "vtt" {
        return Err(Error::invalid("Choose an .srt or .vtt file."));
    }
    let raw = tokio::fs::read(upload.file.path()).await?;
    let text = String::from_utf8(raw).map_err(|_| Error::invalid("The subtitles must be UTF-8 text."))?;
    let (meta, mut cues) = if ext == "srt" { (vec![], subtitle::parse_srt(&text)) } else { subtitle::parse_vtt(&text) };
    if cues.is_empty() {
        return Err(Error::invalid("That file has no subtitles in it."));
    }
    let all: String = cues.iter().flat_map(|c| c.lines.iter()).map(String::as_str).collect::<Vec<_>>().join("\n");
    let req = TranslateRequest {
        text: all,
        target: fields.get("target").cloned().unwrap_or_default(),
        source: fields.get("source").cloned(),
        context: fields.get("context").cloned(),
        model: fields.get("model").cloned(),
    };
    let base = req.job(&st, &access)?;
    let ep = access.chat(&st)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("subtitles").to_owned();
    let lang_tag: String = base.target.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let filename = format!("{stem}.{}.{ext}", if lang_tag.is_empty() { "translated" } else { &lang_tag });
    let _ = upload.len;
    let stream = async_stream::stream! {
        let total = cues.len();
        for (i, cue) in cues.iter_mut().enumerate() {
            let job = Job { text: cue.lines.join("\n"), ..base.clone() };
            match translate::translate_whole(&st.http, &ep, &job, &st.cfg.app.tuning).await {
                Ok(t) => {
                    let t = t.trim();
                    if !t.is_empty() {
                        cue.lines = t.lines().map(str::to_owned).collect();
                    }
                    yield event("progress", &serde_json::json!({ "done": i + 1, "total": total }));
                }
                Err(e) => {
                    yield error_event(&e);
                    return;
                }
            }
        }
        let content = if ext == "srt" { subtitle::render_srt(&cues) } else { subtitle::render_vtt(&meta, &cues) };
        yield event("done", &serde_json::json!({ "filename": filename, "content": content }));
    };
    Ok(sse(stream))
}

#[derive(Deserialize, ToSchema)]
pub struct SpeechRequest {
    pub text: String,
    /// A language listed in `speech_languages` of `/api/v1/models`.
    #[schema(example = "de")]
    pub language: String,
}

/// Read text aloud: MP3 audio in the language's configured voice.
#[utoipa::path(
    post,
    path = "/api/v1/speech",
    tag = "speech",
    request_body = SpeechRequest,
    responses(
        (status = 200, description = "MP3 audio.", content_type = "audio/mpeg"),
        (status = 422, description = "No voice for that language (`INVALID`).", body = ErrorBody),
        (status = 503, description = "No speech endpoint (`NOT_CONFIGURED`).", body = ErrorBody),
    )
)]
async fn speak(State(st): State<AppState>, access: Access, ApiJson(req): ApiJson<SpeechRequest>) -> Result<Response, Error> {
    if req.text.trim().is_empty() {
        return Err(Error::invalid("There is no text to read aloud."));
    }
    access.check_len(&st, &req.text)?;
    let ep = access.speech(&st)?;
    let (voice, model) = access.voice(&st, &req.language)?;
    let chunk_bytes = st.cfg.app.speech.as_ref().map_or(0, |s| s.chunk_bytes);
    let audio = speech::synthesize(&st.http, &ep, voice, model, &req.text, chunk_bytes).await?;
    Ok(([(header::CONTENT_TYPE, "audio/mpeg")], audio).into_response())
}

#[derive(Deserialize, ToSchema)]
pub struct SaveRequest {
    pub text: String,
}

#[derive(Serialize, ToSchema)]
pub struct SaveResponse {
    /// The new paste's address.
    pub url: String,
}

/// Save text as an unlisted paste on the server's BitVault.
#[utoipa::path(
    post,
    path = "/api/v1/bitvault",
    tag = "translate",
    request_body = SaveRequest,
    responses(
        (status = 200, body = SaveResponse),
        (status = 503, description = "No BitVault configured (`NOT_CONFIGURED`).", body = ErrorBody),
    )
)]
async fn save_to_bitvault(State(st): State<AppState>, _access: Access, ApiJson(req): ApiJson<SaveRequest>) -> Result<Json<SaveResponse>, Error> {
    let bv = st.cfg.app.bitvault.as_ref().ok_or(Error::NotConfigured("BitVault"))?;
    Ok(Json(SaveResponse { url: bitvault::save(&st.http, bv, &req.text).await? }))
}

// ---------------------------------------------------------------------------------------------
// Assembly

pub fn upload_limit(st: &AppState) -> usize {
    usize::try_from(st.cfg.app.upload_max_mb.saturating_mul(1 << 20)).unwrap_or(usize::MAX)
}

pub struct Routes {
    /// Multipart uploads, which may be large and slow.
    pub uploads: Router<AppState>,
    pub rest: Router<AppState>,
    pub spec: utoipa::openapi::OpenApi,
}

pub fn assemble() -> Routes {
    let (uploads, upload_spec) =
        OpenApiRouter::new().routes(routes!(transcribe_file)).routes(routes!(translate_subtitles)).split_for_parts();
    let (rest, mut spec) = OpenApiRouter::with_openapi(Doc::openapi())
        .routes(routes!(health))
        .routes(routes!(list_languages))
        .routes(routes!(list_models))
        .routes(routes!(translate_text))
        .routes(routes!(translate_stream))
        .routes(routes!(translate_paragraphs))
        .routes(routes!(detect_language))
        .routes(routes!(speak))
        .routes(routes!(save_to_bitvault))
        .split_for_parts();
    spec.merge(upload_spec);
    Routes { uploads, rest, spec }
}

pub fn routes() -> Result<Routes, Error> {
    let Routes { uploads, rest, spec } = assemble();
    let json = Bytes::from(spec.to_json().map_err(Error::internal)?);
    let rest = rest.route(
        "/api/v1/openapi.json",
        get(move || {
            let json = json.clone();
            async move { ([(header::CONTENT_TYPE, "application/json")], json) }
        }),
    );
    let limiter = Arc::new(RateLimiter::new(120, 30));
    Ok(Routes {
        uploads: uploads.route_layer(ratelimit::layer(limiter.clone())),
        rest: rest.route_layer(ratelimit::layer(limiter)),
        spec,
    })
}
